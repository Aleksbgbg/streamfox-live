use chrono::Local;
use colored::{Color, ColoredString, Colorize};
use log::{Level, Log, Metadata, Record};
use std::io::{self, Write};
#[cfg(not(unix))]
pub use webrtc_logs_not_unix::*;
#[cfg(unix)]
pub use webrtc_logs_unix::*;

#[cfg(unix)]
mod webrtc_logs_unix {
  use super::StdoutLogger;
  use crate::Args;
  use log::{Log, Metadata, Record, SetLoggerError};
  use std::fs::File;
  use std::io::Write;
  use std::ops::DerefMut;
  use std::sync::Mutex;
  use thiserror::Error;

  #[derive(Debug, Error)]
  pub enum InitWebRtcLogsError {
    #[error("could not convert file descriptor to file: {0}")]
    ConvertFdToFile(#[from] filedescriptor::Error),
    #[error("could not create journald logger: {0}")]
    CreateJournaldLogger(std::io::Error),
    #[error("could not set logger: {0}")]
    SetLoggerError(#[from] SetLoggerError),
  }

  pub fn init(args: &Args) -> Result<(), InitWebRtcLogsError> {
    use filedescriptor::FileDescriptor;
    use systemd_journal_logger::JournalLog;

    if let Some(log_level) = args.webrtc_log_level {
      log::set_max_level(log_level.into());

      if let Some(fd) = args.webrtc_log_fd {
        log::set_boxed_logger(Box::new(FileDescriptorLogger {
          file: Mutex::new(FileDescriptor::new(fd).as_file()?),
        }))?;
      } else if systemd_journal_logger::connected_to_journal() {
        JournalLog::new()
          .map_err(InitWebRtcLogsError::CreateJournaldLogger)?
          .install()?;
      } else {
        log::set_boxed_logger(Box::new(StdoutLogger))?;
      }
    }

    Ok(())
  }

  struct FileDescriptorLogger {
    file: Mutex<File>,
  }

  impl Log for FileDescriptorLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
      true
    }

    fn log(&self, record: &Record) {
      let mut file = self.file.lock().unwrap();
      super::write_log(file.deref_mut(), record).expect("could not write to log file");
    }

    fn flush(&self) {
      self
        .file
        .lock()
        .unwrap()
        .flush()
        .expect("could not flush log file");
    }
  }
}

#[cfg(not(unix))]
mod webrtc_logs_not_unix {
  use super::StdoutLogger;
  use crate::Args;
  use log::SetLoggerError;
  use thiserror::Error;

  #[derive(Debug, Error)]
  pub enum InitWebRtcLogsError {
    #[error("could not set logger: {0}")]
    SetLoggerError(#[from] SetLoggerError),
  }

  pub fn init(args: &Args) -> Result<(), InitWebRtcLogsError> {
    if let Some(log_level) = args.webrtc_log_level {
      log::set_max_level(log_level.into());
      log::set_boxed_logger(Box::new(StdoutLogger))?;
    }

    Ok(())
  }
}

struct StdoutLogger;

impl Log for StdoutLogger {
  fn enabled(&self, _metadata: &Metadata) -> bool {
    true
  }

  fn log(&self, record: &Record) {
    write_log(&mut io::stdout(), record).expect("could not write to stdout");
  }

  fn flush(&self) {
    io::stdout().flush().expect("could not flush stdout");
  }
}

fn write_log(file: &mut impl Write, record: &Record) -> Result<(), std::io::Error> {
  writeln!(
    file,
    "[{}] {:5} <{}> {}",
    Local::now().format("%H:%M:%S.%3f"),
    record.level(),
    record.target().italic(),
    to_colored_string(
      record.args().to_string(),
      match record.level() {
        Level::Error => Color::Red,
        Level::Warn => Color::Yellow,
        Level::Info => Color::Cyan,
        Level::Debug => Color::Blue,
        Level::Trace => Color::White,
      }
    )
  )
}

fn to_colored_string(string: String, color: Color) -> ColoredString {
  let mut colored_string = ColoredString::from(string);
  colored_string.fgcolor = Some(color);
  colored_string
}
