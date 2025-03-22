use axum::Router;
use std::net::SocketAddr;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::{select, signal};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::{Level, error, info};

#[derive(Debug, Error)]
enum AppError {
  #[error("could not bind to network interface: {0}")]
  BindTcpListener(std::io::Error),
  #[error("could not get TCP listener address: {0}")]
  GetListenerAddress(std::io::Error),
  #[error("could not start Axum server: {0}")]
  ServeApp(std::io::Error),
}

#[tokio::main]
async fn start() -> Result<(), AppError> {
  let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 8001)))
    .await
    .map_err(AppError::BindTcpListener)?;

  let app = Router::new().layer(
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
    .with_graceful_shutdown(shutdown_signal())
    .await
    .map_err(AppError::ServeApp)?;

  Ok(())
}

fn main() {
  tracing_subscriber::fmt()
    .with_target(false)
    .compact()
    .with_max_level(Level::DEBUG)
    .init();

  info!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

  match start() {
    Ok(_) => info!("app exited successfully"),
    Err(err) => error!("app exited due to error: {}", err),
  }
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
