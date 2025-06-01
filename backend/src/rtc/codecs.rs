use serde_repr::{Deserialize_repr, Serialize_repr};
use std::str::FromStr;
use strum::{EnumString, IntoStaticStr};
use thiserror::Error;

#[derive(Clone, Copy, PartialEq, EnumString, IntoStaticStr, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum VideoCodec {
  H264,
  H265,
  VP8,
  VP9,
  AV1,
}

pub enum Codec {
  SupportedVideo(VideoCodec),
  UnsupportedVideo,
  Audio,
}

#[derive(Debug, Error)]
pub enum MimeTypeToCodecError {
  #[error("mime type does not contain a media type and a codec separated with a slash")]
  InvalidMimeType,
  #[error("track type '{0}' is not video or audio")]
  InvalidTrackType(String),
}

pub fn mime_type_to_codec(mime_type: &str) -> Result<Codec, MimeTypeToCodecError> {
  let parts: Vec<&str> = mime_type.split("/").collect();

  if !parts.len() == 2 {
    return Err(MimeTypeToCodecError::InvalidMimeType);
  }

  let media_type = parts[0];
  let codec = parts[1];

  match media_type {
    "video" => Ok(
      VideoCodec::from_str(codec)
        .ok()
        .map(Codec::SupportedVideo)
        .unwrap_or(Codec::UnsupportedVideo),
    ),
    "audio" => Ok(Codec::Audio),
    value => Err(MimeTypeToCodecError::InvalidTrackType(value.into())),
  }
}
