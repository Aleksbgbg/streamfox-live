mod controllers;

use crate::controllers::room;
use crate::controllers::room::Room;
use axum::{Router, routing};
use clap::Parser;
use dashmap::DashMap;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::{select, signal};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::{Level, error, info};
use webrtc::api::setting_engine::SettingEngine;
use webrtc::ice::udp_network::{EphemeralUDP, UDPNetwork};
use webrtc::ice_transport::ice_candidate_type::RTCIceCandidateType;

/// WebRTC screen sharing server
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
  /// Public IP address to use for the WebRTC ICE host candidate
  #[arg(long)]
  public_ip: String,

  /// Minimum UDP port to use for WebRTC connections (inclusive)
  #[arg(long)]
  port_min: u16,

  /// Maximum UDP port to use for WebRTC connections (inclusive)
  #[arg(long)]
  port_max: u16,
}

#[derive(Debug, Error)]
enum AppError {
  #[error("could not bind to network interface: {0}")]
  BindTcpListener(std::io::Error),
  #[error("could not create ephemeral UDP port range: {0}")]
  CreateEphemeralUdpPortRange(webrtc::ice::Error),
  #[error("could not get path to current executable: {0}")]
  GetCurrentExe(std::io::Error),
  #[error("could not get frontend static files directory")]
  GetFrontendDir,
  #[error("could not get TCP listener address: {0}")]
  GetListenerAddress(std::io::Error),
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

fn create_webrtc_app_config(args: &Args) -> Result<WebRtcAppConfig, AppError> {
  let mut settings = SettingEngine::default();
  settings.set_nat_1to1_ips(vec![args.public_ip.clone()], RTCIceCandidateType::Host);
  settings.set_udp_network(UDPNetwork::Ephemeral(
    EphemeralUDP::new(args.port_min, args.port_max)
      .map_err(AppError::CreateEphemeralUdpPortRange)?,
  ));

  Ok(WebRtcAppConfig { settings })
}

#[tokio::main]
async fn start(args: &Args) -> Result<(), AppError> {
  let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 8001)))
    .await
    .map_err(AppError::BindTcpListener)?;

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
      webrtc: create_webrtc_app_config(args)?,
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

  info!(
    "[TCP] HTTP listening on {}",
    listener
      .local_addr()
      .map_err(AppError::GetListenerAddress)?
  );
  info!(
    "[UDP] WebRTC listening on {}:[{}, {}]",
    args.public_ip, args.port_min, args.port_max,
  );

  axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await
    .map_err(AppError::ServeApp)?;

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
