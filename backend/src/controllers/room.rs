use crate::AppState;
use crate::controllers::errors::{HandlerError, ValidatedJson};
use crate::rtc::codecs::VideoCodec;
use crate::rtc::message::Message;
use crate::rtc::room::Room;
use crate::rtc::session::SessionId;
use crate::rtc::stream::StreamId;
use axum::Json;
use axum::extract::{OriginalUri, Path, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock};
use validator::Validate;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::{APIBuilder, interceptor_registry};
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::sdp_type::RTCSdpType;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;

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
  supported_video_codecs: Vec<VideoCodec>,
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
          supported_video_codecs: request.supported_video_codecs,
          peer_connection,
          response: sender,
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

  let mut gather_complete = peer_connection.gathering_complete_promise().await;

  let mut offer = RTCSessionDescription::default();
  offer.sdp_type = RTCSdpType::Offer;
  offer.sdp = sdp;
  peer_connection
    .set_remote_description(offer)
    .await
    .map_err(HandlerError::SetRemoteDescription)?;

  let transceivers = peer_connection.get_transceivers().await;

  if transceivers.is_empty() {
    return Err(HandlerError::NoTransceivers);
  }

  if !transceivers
    .iter()
    .any(|transceiver| transceiver.kind() == RTPCodecType::Video)
  {
    return Err(HandlerError::NoVideoTransceivers);
  }

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
          peer_connection,
          response: sender,
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
