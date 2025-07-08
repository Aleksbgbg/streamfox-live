mod args;
mod controllers;
mod debug;
mod refcount;
mod rtc;
#[cfg(unix)]
mod unix_socket;

use crate::args::{Args, HttpSocket, WebRtcPortMapping};
use crate::controllers::room;
use crate::debug::webrtc_logs;
use crate::debug::webrtc_logs::InitWebRtcLogsError;
use crate::rtc::room::Room;
use axum::{Router, routing};
use clap::Parser;
use dashmap::DashMap;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tokio::net::{TcpListener, UdpSocket};
use tokio::{select, signal};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::{Level, error, info};
use webrtc::api::setting_engine::SettingEngine;
use webrtc::ice::udp_mux::{UDPMuxDefault, UDPMuxParams};
use webrtc::ice::udp_network::{EphemeralUDP, UDPNetwork};
use webrtc::ice_transport::ice_candidate_type::RTCIceCandidateType;

const IP_V4_UNSPECIFIED_ADDRESS: [u8; 4] = [0, 0, 0, 0];

#[derive(Debug, Error)]
enum AppError {
  #[error(transparent)]
  InitLogsError(#[from] InitWebRtcLogsError),
  #[error("could not bind UDP socket: {0}")]
  BindUdpSocket(std::io::Error),
  #[error("could not create ephemeral UDP port range: {0}")]
  CreateEphemeralUdpPortRange(webrtc::ice::Error),
  #[error("could not get path to current executable: {0}")]
  GetCurrentExe(std::io::Error),
  #[error("could not get frontend static files directory")]
  GetFrontendDir,
  #[error("could not bind TCP socket: {0}")]
  BindTcpListener(std::io::Error),
  #[error("could not get TCP listener address: {0}")]
  GetListenerAddress(std::io::Error),
  #[cfg(unix)]
  #[error("could not bind Unix socket: {0}")]
  CreateUnixSocket(#[from] crate::unix_socket::CreateUnixSocketError),
  #[error("could not start Axum server: {0}")]
  ServeApp(std::io::Error),
}

#[derive(Default)]
struct WebRtcAppConfig {
  settings: SettingEngine,
}

#[derive(Default)]
struct AppState {
  webrtc: WebRtcAppConfig,
  rooms: Arc<DashMap<String, Room>>,
}

async fn create_webrtc_app_config(args: &Args) -> Result<WebRtcAppConfig, AppError> {
  let mut settings = SettingEngine::default();
  settings.set_nat_1to1_ips(vec![args.public_ip.clone()], RTCIceCandidateType::Host);
  settings.set_udp_network(match args.webrtc_ports() {
    WebRtcPortMapping::SinglePort { port_mux } => {
      let udp_socket = UdpSocket::bind(SocketAddr::from((IP_V4_UNSPECIFIED_ADDRESS, port_mux)))
        .await
        .map_err(AppError::BindUdpSocket)?;
      UDPNetwork::Muxed(UDPMuxDefault::new(UDPMuxParams::new(udp_socket)))
    }
    WebRtcPortMapping::PortRange { port_min, port_max } => UDPNetwork::Ephemeral(
      EphemeralUDP::new(port_min, port_max).map_err(AppError::CreateEphemeralUdpPortRange)?,
    ),
  });

  Ok(WebRtcAppConfig { settings })
}

#[tokio::main]
async fn start(args: &Args) -> Result<(), AppError> {
  webrtc_logs::init(args)?;

  let api = Router::new()
    .route("/room/validate-name", routing::post(room::validate_name))
    .route("/room/{name}/session", routing::post(room::create_session))
    .route(
      "/room/{name}/session/{session_id}",
      routing::patch(room::trickle_ice_candidate),
    )
    .route(
      "/room/{name}/session/{session_id}",
      routing::post(room::renegotiate_session),
    )
    .route("/room/{name}/stream", routing::post(room::create_stream))
    .route(
      "/room/{name}/stream/{stream_id}",
      routing::delete(room::destroy_stream),
    )
    .with_state(Arc::new(AppState {
      webrtc: create_webrtc_app_config(args).await?,
      rooms: Arc::new(DashMap::default()),
    }));
  let app = Router::new()
    .fallback_service({
      let mut path = env::current_exe().map_err(AppError::GetCurrentExe)?;
      path.pop();
      path.push("frontend");
      path.push("index.html");

      ServeDir::new(path.parent().ok_or(AppError::GetFrontendDir)?).fallback(ServeFile::new(path))
    })
    .nest("/api", api)
    .layer(
      TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO)),
    );

  let serve = match args.http_socket() {
    HttpSocket::Tcp { port } => {
      let listener = TcpListener::bind(SocketAddr::from((IP_V4_UNSPECIFIED_ADDRESS, port)))
        .await
        .map_err(AppError::BindTcpListener)?;

      info!(
        "[ HTTP ] TCP socket listening on {}",
        listener
          .local_addr()
          .map_err(AppError::GetListenerAddress)?
      );

      axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .into_future()
    }
    #[cfg(unix)]
    HttpSocket::Unix { path } => {
      use crate::unix_socket::UnixSocket;

      let socket = UnixSocket::new(path).await?;

      info!(
        "[ HTTP ] Unix domain socket listening at {}",
        socket.path().display()
      );

      axum::serve(socket, app)
        .with_graceful_shutdown(shutdown_signal())
        .into_future()
    }
  };

  info!(
    "[WebRTC] UDP socket listening on {}:{}",
    args.public_ip,
    match args.webrtc_ports() {
      WebRtcPortMapping::SinglePort { port_mux } => port_mux.to_string(),
      WebRtcPortMapping::PortRange { port_min, port_max } => format!("[{port_min}, {port_max}]"),
    }
  );

  serve.await.map_err(AppError::ServeApp)?;

  Ok(())
}

fn main() {
  let args = Args::parse();

  tracing_subscriber::fmt()
    .with_target(false)
    .compact()
    .with_max_level(Level::DEBUG)
    .init();

  info!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

  match start(&args) {
    Ok(_) => info!("app exited successfully"),
    Err(err) => error!("app exited due to error: {}", err),
  };
}

async fn shutdown_signal() {
  let ctrl_c = async {
    signal::ctrl_c()
      .await
      .expect("failed to install Ctrl+C handler");
  };

  #[cfg(unix)]
  let terminate = async {
    signal::unix::signal(signal::unix::SignalKind::terminate())
      .expect("failed to install SIGTERM handler")
      .recv()
      .await;
  };
  #[cfg(not(unix))]
  let terminate = std::future::pending::<()>();

  select! {
    _ = ctrl_c => {},
    _ = terminate => {},
  }
}
