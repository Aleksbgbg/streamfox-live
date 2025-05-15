use crate::AppState;
use crate::controllers::errors::{HandlerError, ValidatedJson};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
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

const THIRD_PARTY_REMOVAL_ERR: &str =
  "another execution context removed this room from the global map";

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
  const MAX_RETRY_COUNT: u32 = 3;
  for _ in 0..MAX_RETRY_COUNT {
    {
      let room = state
        .rooms
        .entry(room_name.clone())
        .or_insert_with(|| Room::spawn(Arc::clone(&state.rooms), room_name.clone()))
        .downgrade();
      room
        .channel
        .send_async(Message::CreateSession {
          peer_connection: Arc::clone(&peer_connection),
          response: sender.clone(),
        })
        .await
        .map_err(|_| HandlerError::SendMessage)?;
    }

    if let Ok(response) = receiver.recv_async().await {
      return Ok((
        StatusCode::CREATED,
        Json(CreateSessionResponse {
          session_id: response?.session_id.to_string(),
          sdp,
        }),
      ));
    }
  }

  Err(HandlerError::SendMessageMaxRetryReached)
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

pub struct Room {
  pub channel: Sender<Message>,
}

impl Room {
  fn spawn(rooms: Arc<DashMap<String, Room>>, name: String) -> Self {
    let (sender, receiver) = flume::unbounded();

    {
      let sender = sender.clone();
      task::spawn(async move {
        let mut room_thread = RoomTask {
          rooms,
          name,
          receiver,
          sender,

          next_session_id: 0,
          sessions: HashMap::new(),
        };

        room_thread.run().await;
      });
    }

    Self { channel: sender }
  }
}

pub enum Message {
  CreateSession {
    peer_connection: Arc<RTCPeerConnection>,
    response: Sender<Result<message_response::CreateSession, HandlerError>>,
  },
  GetSessionPeerConnection {
    session_id: SessionId,
    response: Sender<Result<message_response::GetSessionPeerConnection, HandlerError>>,
  },
  EstablishSession {
    session_id: SessionId,
    data_channel: Arc<RTCDataChannel>,
  },
  DestroySession {
    session_id: SessionId,
  },
  Exit,
}

mod message_response {
  use crate::controllers::room::SessionId;
  use std::sync::Arc;
  use webrtc::peer_connection::RTCPeerConnection;

  pub struct CreateSession {
    pub session_id: SessionId,
  }

  pub struct GetSessionPeerConnection {
    pub peer_connection: Arc<RTCPeerConnection>,
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

#[derive(Serialize_repr)]
#[repr(u8)]
enum EventType {
  UserJoined,
  UserLeft,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Event {
  r#type: EventType,
}

struct RoomTask {
  rooms: Arc<DashMap<String, Room>>,
  name: String,
  receiver: Receiver<Message>,
  sender: Sender<Message>,

  next_session_id: SessionId,
  sessions: HashMap<SessionId, Session>,
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
        } => self.create_session(peer_connection, response).await,
        Message::GetSessionPeerConnection {
          session_id,
          response,
        } => self.get_session_peer_connection(session_id, response).await,
        Message::EstablishSession {
          session_id,
          data_channel,
        } => self.establish_session(session_id, data_channel).await,
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
    {
      let session = self
        .sessions
        .get_mut(&session_id)
        .expect("attempt to establish session which has been destroyed");
      session.data_channel = Some(data_channel);
    }

    self
      .multicast(
        session_id,
        &Event {
          r#type: EventType::UserJoined,
        },
      )
      .await;

    let session = self.sessions.get(&session_id).unwrap();
    for other in self.sessions.values() {
      if (!other.established()) || (session_id == other.id) {
        continue;
      }

      self
        .unicast(
          session,
          &Event {
            r#type: EventType::UserJoined,
          },
        )
        .await;
    }
  }

  async fn destroy_session(&mut self, session_id: SessionId) {
    if let Some(session) = self.sessions.remove(&session_id) {
      let _ = session.peer_connection.close().await;

      self
        .multicast(
          session_id,
          &Event {
            r#type: EventType::UserLeft,
          },
        )
        .await;

      if self.sessions.is_empty() {
        self.sender.send_async(Message::Exit).await.unwrap();
      }
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
