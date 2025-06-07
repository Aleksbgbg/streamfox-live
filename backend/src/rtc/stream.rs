use std::sync::Arc;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;

pub type StreamId = usize;

pub struct Stream {
  pub id: StreamId,
  pub established: bool,
  pub peer_connection: Arc<RTCPeerConnection>,
  pub tracks: Vec<Arc<TrackLocalStaticRTP>>,
}
