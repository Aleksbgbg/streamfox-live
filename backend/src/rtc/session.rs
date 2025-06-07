use std::sync::Arc;
use webrtc::data_channel::RTCDataChannel;
use webrtc::peer_connection::RTCPeerConnection;

pub type SessionId = usize;

pub struct Session {
  pub id: SessionId,
  pub peer_connection: Arc<RTCPeerConnection>,
  pub data_channel: Option<Arc<RTCDataChannel>>,
}

impl Session {
  pub fn established(&self) -> bool {
    self.data_channel.is_some()
  }
}
