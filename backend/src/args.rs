use clap::{Parser, ValueEnum};
use log::LevelFilter;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LogLevelFilter {
  Error,
  Warn,
  Info,
  Debug,
  Trace,
}

impl From<LogLevelFilter> for LevelFilter {
  fn from(value: LogLevelFilter) -> Self {
    match value {
      LogLevelFilter::Error => LevelFilter::Error,
      LogLevelFilter::Warn => LevelFilter::Warn,
      LogLevelFilter::Info => LevelFilter::Info,
      LogLevelFilter::Debug => LevelFilter::Debug,
      LogLevelFilter::Trace => LevelFilter::Trace,
    }
  }
}

/// WebRTC screen sharing server
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
pub struct Args {
  /// Public IP address to use for the WebRTC ICE host candidate
  #[arg(long)]
  pub public_ip: String,

  /// Minimum UDP port to use for WebRTC connections (inclusive)
  #[arg(long)]
  pub port_min: u16,

  /// Maximum UDP port to use for WebRTC connections (inclusive)
  #[arg(long)]
  pub port_max: u16,

  /// Emit webrtc-rs logs that are at the specified verbosity or lower
  ///
  /// Logs will be emitted:
  ///
  ///  - to stdout by default
  ///
  ///  - to journald when running as a systemd service
  ///
  ///  - to the specified file descriptor if `--webrtc-log-fd` is used
  #[arg(long, value_enum)]
  pub webrtc_log_level: Option<LogLevelFilter>,

  /// Emit webrtc-rs logs to the specified file descriptor
  #[cfg(unix)]
  #[arg(long, requires = "webrtc_log_level")]
  pub webrtc_log_fd: Option<std::os::fd::RawFd>,
}
