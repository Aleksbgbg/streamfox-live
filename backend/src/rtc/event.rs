use crate::rtc::stream::StreamId;
use serde::Serialize;
use serde_repr::Serialize_repr;

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
pub struct Event {
  r#type: EventType,
  stream_started_payload: Option<StreamStartedPayload>,
  stream_ended_payload: Option<StreamEndedPayload>,
}

impl Event {
  fn default(r#type: EventType) -> Self {
    Self {
      r#type,
      stream_started_payload: Default::default(),
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

  pub fn new_stream_ended(stream_id: StreamId) -> Self {
    Self {
      stream_ended_payload: Some(StreamEndedPayload { stream_id }),
      ..Self::default(EventType::StreamEnded)
    }
  }
}
