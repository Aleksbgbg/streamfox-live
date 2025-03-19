use crate::controllers::errors::ValidatedJson;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use regex::Regex;
use serde::Deserialize;
use std::sync::LazyLock;
use validator::Validate;

static REGEX_ROOM_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z-]*$").unwrap());

#[derive(Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ValidateNameRequest {
  #[validate(length(min = 2, max = 256))]
  #[validate(regex(
          path = *REGEX_ROOM_NAME,
          message = "contain only lowercase letters a-z and dashes (-)",
  ))]
  name: String,
}

pub async fn validate_name(
  ValidatedJson(_body): ValidatedJson<ValidateNameRequest>,
) -> impl IntoResponse {
  StatusCode::NO_CONTENT
}
