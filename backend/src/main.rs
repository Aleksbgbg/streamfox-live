use axum::{routing, Router};
use std::net::SocketAddr;
use thiserror::Error;
use tokio::net::TcpListener;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::{error, info, Level};

#[derive(Error, Debug)]
enum AppSuccess {
  #[error("all operations completed")]
  Completed,
}

#[derive(Error, Debug)]
enum AppError {
  #[error("could not bind to network interface: {0}")]
  BindTcpListener(std::io::Error),
  #[error("could not get TCP listener address: {0}")]
  GetListenerAddress(std::io::Error),
  #[error("could not start Axum server: {0}")]
  ServeApp(std::io::Error),
}

#[tokio::main]
async fn start() -> Result<AppSuccess, AppError> {
  let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 8000)))
    .await
    .map_err(AppError::BindTcpListener)?;

  let api = Router::new().route("/hello", routing::get(|| async { "Hello, world!" }));
  let app = Router::new().nest("/api", api).layer(
    TraceLayer::new_for_http()
      .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
      .on_response(DefaultOnResponse::new().level(Level::INFO)),
  );

  info!(
    "backend listening on {}",
    listener
      .local_addr()
      .map_err(AppError::GetListenerAddress)?
  );

  axum::serve(listener, app)
    .await
    .map_err(AppError::ServeApp)?;

  Ok(AppSuccess::Completed)
}

fn main() {
  tracing_subscriber::fmt()
    .with_target(false)
    .compact()
    .with_max_level(Level::DEBUG)
    .init();

  info!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

  match start() {
    Ok(success) => info!("app exited successfully: {}", success),
    Err(err) => error!("app exited due to error: {}", err),
  }
}
