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

pub enum HttpSocket<#[cfg(unix)] 'a> {
  Tcp {
    port: u16,
  },
  #[cfg(unix)]
  Unix {
    path: &'a std::path::Path,
  },
}

pub enum WebRtcPortMapping {
  /// Multiplex all connections on the specified port
  SinglePort { port_mux: u16 },
  /// Accept connections to all ports in the range [port_min, port_max]
  PortRange { port_min: u16, port_max: u16 },
}

/// WebRTC screen sharing server
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
pub struct Args {
  /// Public IP address to use for the WebRTC ICE host candidate
  #[arg(long)]
  pub public_ip: String,

  /// Accept HTTP requests on the specified TCP port
  #[cfg(windows)]
  #[arg(long)]
  http_port: u16,

  /// Accept HTTP requests on the specified TCP port
  #[cfg(unix)]
  #[arg(long, required_unless_present = "http_unix_socket")]
  http_port: Option<u16>,

  /// Accept HTTP requests on the specified Unix socket path
  #[cfg(unix)]
  #[arg(long, conflicts_with = "http_port")]
  http_unix_socket: Option<std::path::PathBuf>,

  /// Multiplex all WebRTC connections on the specified UDP port
  #[arg(long, required_unless_present = "webrtc_port_min")]
  webrtc_port_mux: Option<u16>,

  /// Minimum UDP port to use for WebRTC connections (inclusive)
  #[arg(long, requires = "webrtc_port_max", conflicts_with = "webrtc_port_mux")]
  webrtc_port_min: Option<u16>,

  /// Maximum UDP port to use for WebRTC connections (inclusive)
  #[arg(long, requires = "webrtc_port_min", conflicts_with = "webrtc_port_mux")]
  webrtc_port_max: Option<u16>,

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

impl Args {
  #[cfg(windows)]
  pub fn http_socket(&self) -> HttpSocket {
    HttpSocket::Tcp {
      port: self.http_port,
    }
  }

  #[cfg(unix)]
  pub fn http_socket(&self) -> HttpSocket<'_> {
    if let Some(port) = self.http_port {
      HttpSocket::Tcp { port }
    } else {
      HttpSocket::Unix {
        path: self.http_unix_socket.as_ref().unwrap(),
      }
    }
  }

  pub fn webrtc_ports(&self) -> WebRtcPortMapping {
    if let Some(port_mux) = self.webrtc_port_mux {
      WebRtcPortMapping::SinglePort { port_mux }
    } else {
      WebRtcPortMapping::PortRange {
        port_min: self.webrtc_port_min.unwrap(),
        port_max: self.webrtc_port_max.unwrap(),
      }
    }
  }
}
