use crate::AppState;
use crate::controllers::errors::{HandlerError, ValidatedJson};
use crate::refcount::{Ref, Refcount};
use axum::Json;
use axum::extract::{OriginalUri, Path, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use bytes::Bytes;
use dashmap::DashMap;
use flume::{Receiver, Sender};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_repr::Serialize_repr;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use tokio::task;
use validator::Validate;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::{APIBuilder, interceptor_registry};
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::ice_transport::ice_connection_state::RTCIceConnectionState;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::sdp_type::RTCSdpType;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};
use webrtc::track::track_remote::TrackRemote;

const THIRD_PARTY_REMOVAL_ERR: &str =
  "another execution context removed this room from the global map";

const STREAM_TRACK_COUNT: usize = 2;

const RTP_BUFFER_SIZE_BYTES: usize = 4096;
const RTCP_BUFFER_SIZE_BYTES: usize = 4096;

static REGEX_ROOM_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z-]*$").unwrap());

#[derive(Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ValidateNameRequest {
  #[validate(length(min = 2, max = 256))]
  #[validate(regex(
          path = *REGEX_ROOM_NAME,
          message = "contain only lowercase letters a-z and dashes (-)",
  ))]
  name: String,
}

pub async fn validate_name(
  ValidatedJson(_body): ValidatedJson<ValidateNameRequest>,
) -> impl IntoResponse {
  StatusCode::NO_CONTENT
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
  sdp: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateSessionResponse {
  session_id: String,
  sdp: String,
}

pub async fn create_session(
  State(state): State<Arc<AppState>>,
  Path(room_name): Path<String>,
  Json(request): Json<CreateSessionRequest>,
) -> Result<impl IntoResponse, HandlerError> {
  let mut engine = MediaEngine::default();
  engine
    .register_default_codecs()
    .map_err(HandlerError::RegisterDefaultCodecs)?;
  let registry = interceptor_registry::register_default_interceptors(Registry::new(), &mut engine)
    .map_err(HandlerError::RegisterDefaultInterceptors)?;
  let api = APIBuilder::new()
    .with_setting_engine(state.webrtc.settings.clone())
    .with_media_engine(engine)
    .with_interceptor_registry(registry)
    .build();

  let peer_connection = Arc::new(
    api
      .new_peer_connection(RTCConfiguration::default())
      .await
      .map_err(HandlerError::CreatePeerConnection)?,
  );

  let mut gather_complete = peer_connection.gathering_complete_promise().await;

  let mut offer = RTCSessionDescription::default();
  offer.sdp_type = RTCSdpType::Offer;
  offer.sdp = request.sdp;
  peer_connection
    .set_remote_description(offer)
    .await
    .map_err(HandlerError::SetRemoteDescription)?;

  let answer = peer_connection
    .create_answer(None)
    .await
    .map_err(HandlerError::CreateAnswer)?;

  peer_connection
    .set_local_description(answer)
    .await
    .map_err(HandlerError::SetLocalDescription)?;

  gather_complete.recv().await;

  let sdp = peer_connection
    .local_description()
    .await
    .ok_or(HandlerError::GetLocalDescription)?
    .sdp;

  let (sender, receiver) = flume::bounded(1);
  loop {
    let room = state
      .rooms
      .entry(room_name.clone())
      .or_insert_with(|| Room::spawn(Arc::clone(&state.rooms), room_name.clone()))
      .downgrade();

    if let Some(r#ref) = room.refcount.try_get() {
      room
        .channel
        .send_async(Message::CreateSession {
          peer_connection: Arc::clone(&peer_connection),
          response: sender.clone(),
          _ref: r#ref,
        })
        .await
        .map_err(|_| HandlerError::SendMessage)?;
      break;
    }
  }

  let response = receiver.recv_async().await??;

  Ok((
    StatusCode::CREATED,
    Json(CreateSessionResponse {
      session_id: response.session_id.to_string(),
      sdp,
    }),
  ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenegotiateSessionRequest {
  sdp: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RenegotiateSessionResponse {
  sdp: String,
}

pub async fn renegotiate_session(
  State(state): State<Arc<AppState>>,
  Path((room_name, session_id)): Path<(String, SessionId)>,
  Json(request): Json<RenegotiateSessionRequest>,
) -> Result<impl IntoResponse, HandlerError> {
  let receiver = {
    let (sender, receiver) = flume::bounded(1);

    let room = state
      .rooms
      .get(&room_name)
      .ok_or_else(|| HandlerError::RoomNotFound(room_name))?;

    room
      .channel
      .send_async(Message::GetSessionPeerConnection {
        session_id,
        response: sender,
      })
      .await
      .map_err(|_| HandlerError::SendMessage)?;

    receiver
  };

  let response = receiver.recv_async().await??;

  let peer_connection = response.peer_connection;

  let mut gather_complete = peer_connection.gathering_complete_promise().await;

  let mut offer = RTCSessionDescription::default();
  offer.sdp_type = RTCSdpType::Offer;
  offer.sdp = request.sdp;
  peer_connection
    .set_remote_description(offer)
    .await
    .map_err(HandlerError::SetRemoteDescription)?;

  let answer = peer_connection
    .create_answer(None)
    .await
    .map_err(HandlerError::CreateAnswer)?;

  peer_connection
    .set_local_description(answer)
    .await
    .map_err(HandlerError::SetLocalDescription)?;

  gather_complete.recv().await;

  let sdp = peer_connection
    .local_description()
    .await
    .ok_or(HandlerError::GetLocalDescription)?
    .sdp;

  Ok(Json(RenegotiateSessionResponse { sdp }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrickleIceCandidateRequest {
  candidate: RTCIceCandidateInit,
}

pub async fn trickle_ice_candidate(
  State(state): State<Arc<AppState>>,
  Path((room_name, session_id)): Path<(String, SessionId)>,
  Json(request): Json<TrickleIceCandidateRequest>,
) -> Result<impl IntoResponse, HandlerError> {
  let receiver = {
    let (sender, receiver) = flume::bounded(1);

    let room = state
      .rooms
      .get(&room_name)
      .ok_or_else(|| HandlerError::RoomNotFound(room_name))?;

    room
      .channel
      .send_async(Message::GetSessionPeerConnection {
        session_id,
        response: sender,
      })
      .await
      .map_err(|_| HandlerError::SendMessage)?;

    receiver
  };

  let response = receiver.recv_async().await??;

  response
    .peer_connection
    .add_ice_candidate(request.candidate)
    .await
    .map_err(HandlerError::AddIceCandidate)?;

  Ok(StatusCode::NO_CONTENT)
}

pub async fn create_stream(
  State(state): State<Arc<AppState>>,
  OriginalUri(path): OriginalUri,
  Path(room_name): Path<String>,
  sdp: String,
) -> Result<impl IntoResponse, HandlerError> {
  let mut engine = MediaEngine::default();
  engine
    .register_default_codecs()
    .map_err(HandlerError::RegisterDefaultCodecs)?;
  let registry = interceptor_registry::register_default_interceptors(Registry::new(), &mut engine)
    .map_err(HandlerError::RegisterDefaultInterceptors)?;
  let api = APIBuilder::new()
    .with_setting_engine(state.webrtc.settings.clone())
    .with_media_engine(engine)
    .with_interceptor_registry(registry)
    .build();

  let peer_connection = Arc::new(
    api
      .new_peer_connection(RTCConfiguration::default())
      .await
      .map_err(HandlerError::CreatePeerConnection)?,
  );

  peer_connection
    .add_transceiver_from_kind(RTPCodecType::Video, None)
    .await
    .map_err(HandlerError::AddVideoTransceiver)?;
  peer_connection
    .add_transceiver_from_kind(RTPCodecType::Audio, None)
    .await
    .map_err(HandlerError::AddAudioTransceiver)?;

  let mut gather_complete = peer_connection.gathering_complete_promise().await;

  let mut offer = RTCSessionDescription::default();
  offer.sdp_type = RTCSdpType::Offer;
  offer.sdp = sdp;
  peer_connection
    .set_remote_description(offer)
    .await
    .map_err(HandlerError::SetRemoteDescription)?;

  let answer = peer_connection
    .create_answer(None)
    .await
    .map_err(HandlerError::CreateAnswer)?;

  peer_connection
    .set_local_description(answer)
    .await
    .map_err(HandlerError::SetLocalDescription)?;

  gather_complete.recv().await;

  let sdp = peer_connection
    .local_description()
    .await
    .ok_or(HandlerError::GetLocalDescription)?
    .sdp;

  let (sender, receiver) = flume::bounded(1);
  loop {
    let room = state
      .rooms
      .entry(room_name.clone())
      .or_insert_with(|| Room::spawn(Arc::clone(&state.rooms), room_name.clone()))
      .downgrade();

    if let Some(r#ref) = room.refcount.try_get() {
      room
        .channel
        .send_async(Message::CreateStream {
          peer_connection: Arc::clone(&peer_connection),
          response: sender.clone(),
          _ref: r#ref,
        })
        .await
        .map_err(|_| HandlerError::SendMessage)?;
      break;
    }
  }

  let response = receiver.recv_async().await??;

  Ok((
    StatusCode::CREATED,
    [(header::LOCATION, format!("{}/{}", path, response.stream_id))],
    sdp,
  ))
}

pub async fn destroy_stream(
  State(state): State<Arc<AppState>>,
  Path((room_name, stream_id)): Path<(String, StreamId)>,
) -> Result<impl IntoResponse, HandlerError> {
  let room = state
    .rooms
    .get(&room_name)
    .ok_or_else(|| HandlerError::RoomNotFound(room_name))?;

  room
    .channel
    .send_async(Message::DestroyStream { stream_id })
    .await
    .map_err(|_| HandlerError::SendMessage)?;

  Ok(StatusCode::NO_CONTENT)
}

pub struct Room {
  refcount: Refcount,
  pub channel: Sender<Message>,
}

impl Room {
  fn spawn(rooms: Arc<DashMap<String, Room>>, name: String) -> Self {
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

pub enum Message {
  CreateSession {
    peer_connection: Arc<RTCPeerConnection>,
    response: Sender<Result<message_response::CreateSession, HandlerError>>,
    _ref: Ref,
  },
  GetSessionPeerConnection {
    session_id: SessionId,
    response: Sender<Result<message_response::GetSessionPeerConnection, HandlerError>>,
  },
  EstablishSession {
    session_id: SessionId,
    data_channel: Arc<RTCDataChannel>,
  },
  CreateStream {
    peer_connection: Arc<RTCPeerConnection>,
    response: Sender<Result<message_response::CreateStream, HandlerError>>,
    _ref: Ref,
  },
  HandleStreamTrack {
    stream_id: StreamId,
    remote_track: Arc<TrackRemote>,
  },
  DestroyStream {
    stream_id: StreamId,
  },
  DestroySession {
    session_id: SessionId,
  },
  Exit,
}

mod message_response {
  use crate::controllers::room::{SessionId, StreamId};
  use std::sync::Arc;
  use webrtc::peer_connection::RTCPeerConnection;

  pub struct CreateSession {
    pub session_id: SessionId,
  }

  pub struct GetSessionPeerConnection {
    pub peer_connection: Arc<RTCPeerConnection>,
  }

  pub struct CreateStream {
    pub stream_id: StreamId,
  }
}

type SessionId = usize;

struct Session {
  id: SessionId,
  peer_connection: Arc<RTCPeerConnection>,
  data_channel: Option<Arc<RTCDataChannel>>,
}

impl Session {
  fn established(&self) -> bool {
    self.data_channel.is_some()
  }
}

type StreamId = usize;

struct Stream {
  id: StreamId,
  established: bool,
  peer_connection: Arc<RTCPeerConnection>,
  tracks: Vec<Arc<TrackLocalStaticRTP>>,
}

#[derive(Serialize_repr)]
#[repr(u8)]
enum EventType {
  UserJoined,
  UserLeft,
  StreamStarted,
  StreamEnded,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamStartedPayload {
  stream_id: StreamId,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamEndedPayload {
  stream_id: StreamId,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Event {
  r#type: EventType,
  stream_started_payload: Option<StreamStartedPayload>,
  stream_ended_payload: Option<StreamEndedPayload>,
}

impl Event {
  fn new_user_joined() -> Self {
    Event {
      r#type: EventType::UserJoined,
      stream_started_payload: None,
      stream_ended_payload: None,
    }
  }

  fn new_user_left() -> Self {
    Event {
      r#type: EventType::UserLeft,
      stream_started_payload: None,
      stream_ended_payload: None,
    }
  }

  fn new_stream_started(stream_id: StreamId) -> Self {
    Event {
      r#type: EventType::StreamStarted,
      stream_started_payload: Some(StreamStartedPayload { stream_id }),
      stream_ended_payload: None,
    }
  }

  fn new_stream_ended(stream_id: StreamId) -> Self {
    Event {
      r#type: EventType::StreamEnded,
      stream_started_payload: None,
      stream_ended_payload: Some(StreamEndedPayload { stream_id }),
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
          peer_connection,
          response,
          ..
        } => self.create_session(peer_connection, response).await,
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
    peer_connection: Arc<RTCPeerConnection>,
    response: Sender<Result<message_response::CreateSession, HandlerError>>,
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
          peer_connection,
          data_channel: None,
        },
      );

      Ok(message_response::CreateSession { session_id })
    };

    let _ = response.send_async(result).await;
  }

  async fn get_session_peer_connection(
    &self,
    session_id: SessionId,
    response: Sender<Result<message_response::GetSessionPeerConnection, HandlerError>>,
  ) {
    let result = if let Some(session) = self.sessions.get(&session_id) {
      Ok(message_response::GetSessionPeerConnection {
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
    for (&stream_id, stream) in &self.streams {
      if !stream.established {
        continue;
      }

      self
        .unicast(session, &Event::new_stream_started(stream_id))
        .await;
      self.add_stream(session, stream).await;
    }
  }

  async fn create_stream(
    &mut self,
    peer_connection: Arc<RTCPeerConnection>,
    response: Sender<Result<message_response::CreateStream, HandlerError>>,
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
          established: false,
          peer_connection,
          tracks: Vec::with_capacity(STREAM_TRACK_COUNT),
        },
      );

      Ok(message_response::CreateStream { stream_id })
    };

    let _ = response.send_async(result).await;
  }

  async fn handle_stream_track(&mut self, stream_id: StreamId, remote_track: Arc<TrackRemote>) {
    if !self.streams.contains_key(&stream_id) {
      return;
    }

    let stream = self.streams.get_mut(&stream_id).unwrap();

    let local_track = Arc::new(TrackLocalStaticRTP::new(
      remote_track.codec().capability,
      remote_track.id(),
      remote_track.stream_id(),
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

    self.broadcast(&Event::new_stream_started(stream_id)).await;

    for session in self.sessions.values() {
      if !session.established() {
        continue;
      }

      self.add_stream(session, stream).await;
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
