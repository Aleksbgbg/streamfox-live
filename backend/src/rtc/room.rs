use crate::controllers::errors::HandlerError;
use crate::refcount::Refcount;
use crate::rtc::codecs::{self, Codec, VideoCodec};
use crate::rtc::event::{Event, StreamFailedError};
use crate::rtc::message::{Message, response};
use crate::rtc::session::{Session, SessionId};
use crate::rtc::stream::{Stream, StreamId};
use bytes::Bytes;
use dashmap::DashMap;
use flume::{Receiver, Sender};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_connection_state::RTCIceConnectionState;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};
use webrtc::track::track_remote::TrackRemote;

const MSID_MAX_LEN: usize = 64;

const THIRD_PARTY_REMOVAL_ERR: &str =
  "another execution context removed this room from the global map";

const STREAM_TRACK_COUNT: usize = 2;

const RTP_BUFFER_SIZE_BYTES: usize = 4096;
const RTCP_BUFFER_SIZE_BYTES: usize = 4096;

pub struct Room {
  pub refcount: Refcount,
  pub channel: Sender<Message>,
}

impl Room {
  pub fn spawn(rooms: Arc<DashMap<String, Room>>, name: String) -> Self {
    let refcount = Refcount::new();
    let (sender, receiver) = flume::unbounded();

    {
      let refcount = refcount.clone();
      let sender = sender.clone();
      task::spawn(async move {
        let mut room_thread = RoomTask {
          rooms,
          refcount,
          name,
          receiver,
          sender,

          next_session_id: 0,
          sessions: HashMap::new(),

          next_stream_id: 0,
          streams: HashMap::new(),
        };

        room_thread.run().await;
      });
    }

    Self {
      refcount,
      channel: sender,
    }
  }
}

struct RoomTask {
  rooms: Arc<DashMap<String, Room>>,
  refcount: Refcount,
  name: String,
  receiver: Receiver<Message>,
  sender: Sender<Message>,

  next_session_id: SessionId,
  sessions: HashMap<SessionId, Session>,

  next_stream_id: StreamId,
  streams: HashMap<StreamId, Stream>,
}

impl RoomTask {
  async fn run(&mut self) {
    loop {
      match self
        .receiver
        .recv_async()
        .await
        .expect(THIRD_PARTY_REMOVAL_ERR)
      {
        Message::CreateSession {
          supported_video_codecs,
          peer_connection,
          response,
          ..
        } => {
          self
            .create_session(supported_video_codecs, peer_connection, response)
            .await
        }
        Message::GetSessionPeerConnection {
          session_id,
          response,
        } => self.get_session_peer_connection(session_id, response).await,
        Message::EstablishSession {
          session_id,
          data_channel,
        } => self.establish_session(session_id, data_channel).await,
        Message::CreateStream {
          peer_connection,
          response,
          ..
        } => self.create_stream(peer_connection, response).await,
        Message::HandleStreamTrack {
          stream_id,
          remote_track,
        } => self.handle_stream_track(stream_id, remote_track).await,
        Message::DestroyStream { stream_id } => self.destroy_stream(stream_id).await,
        Message::DestroySession { session_id } => self.destroy_session(session_id).await,
        Message::Exit => break,
      }
    }

    self
      .rooms
      .remove(&self.name)
      .expect(THIRD_PARTY_REMOVAL_ERR);
  }

  async fn create_session(
    &mut self,
    supported_video_codecs: Vec<VideoCodec>,
    peer_connection: Arc<RTCPeerConnection>,
    response: Sender<Result<response::CreateSession, HandlerError>>,
  ) {
    let result = {
      self.next_session_id += 1;
      let session_id = self.next_session_id;

      {
        let sender = self.sender.clone();
        peer_connection.on_ice_connection_state_change(Box::new(move |connection_state| {
          Box::pin({
            let sender = sender.clone();
            async move {
              match connection_state {
                RTCIceConnectionState::Disconnected
                | RTCIceConnectionState::Failed
                | RTCIceConnectionState::Closed => {
                  let _ = sender
                    .send_async(Message::DestroySession { session_id })
                    .await;
                }
                _ => {}
              };
            }
          })
        }));
      }

      {
        let sender = self.sender.clone();
        peer_connection.on_data_channel(Box::new(move |data_channel| {
          data_channel.on_close({
            let sender = sender.clone();
            Box::new(move || {
              let sender = sender.clone();
              Box::pin(async move {
                let _ = sender
                  .send_async(Message::DestroySession { session_id })
                  .await;
              })
            })
          });
          Box::pin({
            let sender = sender.clone();
            async move {
              let _ = sender
                .send_async(Message::EstablishSession {
                  session_id,
                  data_channel,
                })
                .await;
            }
          })
        }));
      }

      self.sessions.insert(
        session_id,
        Session {
          id: session_id,
          supported_video_codecs,
          peer_connection,
          data_channel: None,
        },
      );

      Ok(response::CreateSession { session_id })
    };

    let _ = response.send_async(result).await;
  }

  async fn get_session_peer_connection(
    &self,
    session_id: SessionId,
    response: Sender<Result<response::GetSessionPeerConnection, HandlerError>>,
  ) {
    let result = if let Some(session) = self.sessions.get(&session_id) {
      Ok(response::GetSessionPeerConnection {
        peer_connection: Arc::clone(&session.peer_connection),
      })
    } else {
      Err(HandlerError::SessionNotFound(session_id))
    };

    let _ = response.send_async(result).await;
  }

  async fn establish_session(&mut self, session_id: SessionId, data_channel: Arc<RTCDataChannel>) {
    if !self.sessions.contains_key(&session_id) {
      return;
    }

    {
      let session = self.sessions.get_mut(&session_id).unwrap();
      session.data_channel = Some(data_channel);
    }

    self.multicast(session_id, &Event::new_user_joined()).await;

    let session = self.sessions.get(&session_id).unwrap();
    for other in self.sessions.values() {
      if (!other.established()) || (session_id == other.id) {
        continue;
      }

      self.unicast(session, &Event::new_user_joined()).await;
    }
    for stream in self.streams.values() {
      if !stream.established {
        continue;
      }

      self.try_add_stream(session, stream).await;
    }
  }

  async fn create_stream(
    &mut self,
    peer_connection: Arc<RTCPeerConnection>,
    response: Sender<Result<response::CreateStream, HandlerError>>,
  ) {
    let result = {
      self.next_stream_id += 1;
      let stream_id = self.next_stream_id;

      {
        let sender = self.sender.clone();
        peer_connection.on_track(Box::new(move |remote_track, _, _| {
          let sender = sender.clone();
          Box::pin(async move {
            let _ = sender
              .send_async(Message::HandleStreamTrack {
                stream_id,
                remote_track,
              })
              .await;
          })
        }));
      }

      {
        let sender = self.sender.clone();
        peer_connection.on_ice_connection_state_change(Box::new(move |connection_state| {
          Box::pin({
            let sender = sender.clone();
            async move {
              match connection_state {
                RTCIceConnectionState::Disconnected
                | RTCIceConnectionState::Failed
                | RTCIceConnectionState::Closed => {
                  let _ = sender
                    .send_async(Message::DestroyStream { stream_id })
                    .await;
                }
                _ => {}
              };
            }
          })
        }));
      }

      self.streams.insert(
        stream_id,
        Stream {
          id: stream_id,
          video_codec: None,
          established: false,
          peer_connection,
          tracks: Vec::with_capacity(STREAM_TRACK_COUNT),
        },
      );

      Ok(response::CreateStream { stream_id })
    };

    let _ = response.send_async(result).await;
  }

  async fn handle_stream_track(&mut self, stream_id: StreamId, remote_track: Arc<TrackRemote>) {
    if !self.streams.contains_key(&stream_id) {
      return;
    }

    let capability = remote_track.codec().capability;
    let stream = self.streams.get_mut(&stream_id).unwrap();

    match codecs::mime_type_to_codec(&capability.mime_type) {
      Ok(codec) => match codec {
        Codec::SupportedVideo(video_codec) => stream.video_codec = Some(video_codec),
        Codec::UnsupportedVideo => {
          self.destroy_stream(stream_id).await;
          return;
        }
        Codec::Audio => {}
      },
      Err(_) => {
        self.destroy_stream(stream_id).await;
        return;
      }
    }

    let local_track = Arc::new(TrackLocalStaticRTP::new(
      capability,
      ensure_unique_id(remote_track.id(), stream_id),
      ensure_unique_id(remote_track.stream_id(), stream_id),
    ));

    {
      let local_track = Arc::clone(&local_track);
      let sender = self.sender.clone();
      task::spawn(async move {
        let mut buffer = [0u8; RTP_BUFFER_SIZE_BYTES];
        while let Ok((packet, _)) = remote_track.read(&mut buffer).await {
          let result = local_track.write_rtp(&packet).await;
          if result.is_err() {
            break;
          }
        }

        let _ = sender
          .send_async(Message::DestroyStream { stream_id })
          .await;
      });
    }

    stream.tracks.push(local_track);

    if stream.tracks.len() != STREAM_TRACK_COUNT {
      return;
    }

    self.establish_stream(stream_id).await;
  }

  async fn establish_stream(&mut self, stream_id: StreamId) {
    {
      let stream = self.streams.get_mut(&stream_id).unwrap();
      stream.established = true;
    }

    let stream = self.streams.get(&stream_id).unwrap();

    for session in self.sessions.values() {
      if !session.established() {
        continue;
      }

      self.try_add_stream(session, stream).await;
    }
  }

  async fn destroy_stream(&mut self, stream_id: StreamId) {
    if let Some(stream) = self.streams.remove(&stream_id) {
      let _ = stream.peer_connection.close().await;

      if stream.established {
        self.broadcast(&Event::new_stream_ended(stream_id)).await;
      }

      self.try_exit().await;
    }
  }

  async fn destroy_session(&mut self, session_id: SessionId) {
    if let Some(session) = self.sessions.remove(&session_id) {
      let _ = session.peer_connection.close().await;

      if session.established() {
        self.multicast(session_id, &Event::new_user_left()).await;
      }

      self.try_exit().await;
    }
  }

  async fn try_exit(&self) {
    if !self.sessions.is_empty() || !self.streams.is_empty() {
      return;
    }

    if !self.refcount.try_release() {
      return;
    }

    self.sender.send_async(Message::Exit).await.unwrap();
  }

  async fn try_add_stream(&self, session: &Session, stream: &Stream) {
    if session
      .supported_video_codecs
      .contains(&stream.video_codec.unwrap())
    {
      self
        .unicast(session, &Event::new_stream_started(stream.id))
        .await;
      self.add_stream(session, stream).await;
    } else {
      self
        .unicast(
          session,
          &Event::new_stream_failed(
            stream.id,
            StreamFailedError::new_unsupported_video_codec(stream.video_codec.unwrap()),
          ),
        )
        .await;
    }
  }

  async fn add_stream(&self, session: &Session, stream: &Stream) {
    for track in &stream.tracks {
      if let Ok(rtp_sender) = session
        .peer_connection
        .add_track(Arc::clone(track) as Arc<dyn TrackLocal + Send + Sync>)
        .await
      {
        let peer_connection = Arc::clone(&stream.peer_connection);
        let sender = self.sender.clone();
        let session_id = session.id;
        task::spawn(async move {
          let mut buffer = [0u8; RTCP_BUFFER_SIZE_BYTES];
          while let Ok((packets, _)) = rtp_sender.read(&mut buffer).await {
            let result = peer_connection.write_rtcp(&packets).await;
            if result.is_err() {
              break;
            }
          }

          let _ = sender
            .send_async(Message::DestroySession { session_id })
            .await;
        });
      } else {
        self
          .sender
          .send_async(Message::DestroyStream {
            stream_id: stream.id,
          })
          .await
          .unwrap();
      }
    }
  }

  async fn broadcast(&self, event: &Event) {
    let message = serialize(event);
    for session in self.sessions.values() {
      self.send(session, &message).await;
    }
  }

  async fn multicast(&self, source: SessionId, event: &Event) {
    let message = serialize(event);
    for (session_id, session) in &self.sessions {
      if *session_id == source {
        continue;
      }

      self.send(session, &message).await;
    }
  }

  async fn unicast(&self, session: &Session, event: &Event) {
    let message = serialize(event);
    self.send(session, &message).await;
  }

  async fn send(&self, session: &Session, message: &Bytes) {
    if let Some(data_channel) = &session.data_channel {
      let result = data_channel.send(message).await;

      if result.is_err() {
        self
          .sender
          .send_async(Message::DestroySession {
            session_id: session.id,
          })
          .await
          .unwrap();
      }
    }
  }
}

fn serialize(event: &Event) -> Bytes {
  Bytes::from(serde_json::to_string(&event).expect("could not convert event to string"))
}

fn ensure_unique_id(id: String, stream_id: StreamId) -> String {
  let mut unique_id = format!("{stream_id}+{id}");
  unique_id.truncate(MSID_MAX_LEN);
  unique_id
}
