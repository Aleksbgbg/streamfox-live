use axum::serve::Listener;
use rustix::fs::Mode;
use rustix::process;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::fs;
use tokio::net::unix::SocketAddr;
use tokio::net::{UnixListener, UnixStream};
use tracing::warn;

#[derive(Debug, Error)]
pub enum CreateUnixSocketError {
  #[error("could not query Unix socket parent directory for existence: {0}")]
  QueryParentDir(std::io::Error),
  #[error("Unix socket parent directory does not exist")]
  ParentDirMissing,
  #[error("Unix socket path does not have a parent directory")]
  NoParentDir,
  #[error("could not canonicalize Unix socket path: {0}")]
  Canonicalize(std::io::Error),
  #[error("Unix socket path does not have a file name")]
  NoFileName,
  #[error("could not query Unix socket for existence: {0}")]
  QuerySocket(std::io::Error),
  #[error("could not remove existing Unix socket: {0}")]
  RemoveSocket(std::io::Error),
  #[error("could not bind Unix listener: {0}")]
  BindUnixListener(std::io::Error),
}

pub struct UnixSocket {
  path: PathBuf,
  listener: UnixListener,
}

impl UnixSocket {
  pub async fn new(path: &Path) -> Result<Self, CreateUnixSocketError> {
    let path = if let Some(parent) = path.parent() {
      if !fs::try_exists(parent)
        .await
        .map_err(CreateUnixSocketError::QueryParentDir)?
      {
        return Err(CreateUnixSocketError::ParentDirMissing);
      }

      let mut canonical_path = fs::canonicalize(parent)
        .await
        .map_err(CreateUnixSocketError::Canonicalize)?;
      canonical_path.push(path.file_name().ok_or(CreateUnixSocketError::NoFileName)?);

      canonical_path
    } else {
      return Err(CreateUnixSocketError::NoParentDir);
    };

    if fs::try_exists(&path)
      .await
      .map_err(CreateUnixSocketError::QuerySocket)?
    {
      fs::remove_file(&path)
        .await
        .map_err(CreateUnixSocketError::RemoveSocket)?;
      warn!("Removed existing Unix socket {}", path.display());
    }

    let umask_original = process::umask(Mode::empty());
    let result = UnixListener::bind(&path).map_err(CreateUnixSocketError::BindUnixListener);
    process::umask(umask_original);

    let listener = result?;

    Ok(Self { path, listener })
  }

  pub fn path(&self) -> &Path {
    &self.path
  }
}

impl Listener for UnixSocket {
  type Io = UnixStream;
  type Addr = SocketAddr;

  #[inline]
  async fn accept(&mut self) -> (Self::Io, Self::Addr) {
    Listener::accept(&mut self.listener).await
  }

  #[inline]
  fn local_addr(&self) -> std::io::Result<Self::Addr> {
    Listener::local_addr(&self.listener)
  }
}

impl Drop for UnixSocket {
  fn drop(&mut self) {
    let _ = std::fs::remove_file(&self.path);
  }
}
