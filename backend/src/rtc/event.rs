use crate::rtc::codecs::VideoCodec;
use crate::rtc::stream::StreamId;
use serde::Serialize;
use serde_repr::Serialize_repr;

#[derive(Serialize_repr)]
#[repr(u8)]
enum EventType {
  UserJoined,
  UserLeft,
  StreamStarted,
  StreamFailed,
  StreamEnded,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamStartedPayload {
  stream_id: StreamId,
}

#[derive(Serialize_repr)]
#[repr(u8)]
enum StreamFailedErrorCode {
  UnsupportedVideoCodec,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnsupportedVideoCodecParams {
  source: VideoCodec,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamFailedError {
  code: StreamFailedErrorCode,
  unsupported_video_codec_params: Option<UnsupportedVideoCodecParams>,
}

impl StreamFailedError {
  fn default(code: StreamFailedErrorCode) -> Self {
    Self {
      code,
      unsupported_video_codec_params: Default::default(),
    }
  }

  pub fn new_unsupported_video_codec(source: VideoCodec) -> Self {
    Self {
      unsupported_video_codec_params: Some(UnsupportedVideoCodecParams { source }),
      ..Self::default(StreamFailedErrorCode::UnsupportedVideoCodec)
    }
  }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamFailedPayload {
  stream_id: StreamId,
  error: StreamFailedError,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamEndedPayload {
  stream_id: StreamId,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
  r#type: EventType,
  stream_started_payload: Option<StreamStartedPayload>,
  stream_failed_payload: Option<StreamFailedPayload>,
  stream_ended_payload: Option<StreamEndedPayload>,
}

impl Event {
  fn default(r#type: EventType) -> Self {
    Self {
      r#type,
      stream_started_payload: Default::default(),
      stream_failed_payload: Default::default(),
      stream_ended_payload: Default::default(),
    }
  }

  pub fn new_user_joined() -> Self {
    Self::default(EventType::UserJoined)
  }

  pub fn new_user_left() -> Self {
    Self::default(EventType::UserLeft)
  }

  pub fn new_stream_started(stream_id: StreamId) -> Self {
    Self {
      stream_started_payload: Some(StreamStartedPayload { stream_id }),
      ..Self::default(EventType::StreamStarted)
    }
  }

  pub fn new_stream_failed(stream_id: StreamId, error: StreamFailedError) -> Self {
    Self {
      stream_failed_payload: Some(StreamFailedPayload { stream_id, error }),
      ..Self::default(EventType::StreamFailed)
    }
  }

  pub fn new_stream_ended(stream_id: StreamId) -> Self {
    Self {
      stream_ended_payload: Some(StreamEndedPayload { stream_id }),
      ..Self::default(EventType::StreamEnded)
    }
  }
}
