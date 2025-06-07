use crate::controllers::errors::HandlerError;
use crate::refcount::Ref;
use crate::rtc::session::SessionId;
use crate::rtc::stream::StreamId;
use flume::Sender;
use std::sync::Arc;
use webrtc::data_channel::RTCDataChannel;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::track::track_remote::TrackRemote;

pub enum Message {
  CreateSession {
    peer_connection: Arc<RTCPeerConnection>,
    response: Sender<Result<response::CreateSession, HandlerError>>,
    _ref: Ref,
  },
  GetSessionPeerConnection {
    session_id: SessionId,
    response: Sender<Result<response::GetSessionPeerConnection, HandlerError>>,
  },
  EstablishSession {
    session_id: SessionId,
    data_channel: Arc<RTCDataChannel>,
  },
  CreateStream {
    peer_connection: Arc<RTCPeerConnection>,
    response: Sender<Result<response::CreateStream, HandlerError>>,
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

pub mod response {
  use crate::rtc::session::SessionId;
  use crate::rtc::stream::StreamId;
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
