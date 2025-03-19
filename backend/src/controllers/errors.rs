use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use convert_case::{Case, Casing};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use thiserror::Error;
use tracing::error;
use validator::{Validate, ValidationErrors, ValidationErrorsKind};

#[derive(Default, Serialize)]
struct Errors {
  generic: Vec<String>,
  specific: HashMap<String, Vec<String>>,
}

impl Errors {
  fn generic(value: String) -> Self {
    let mut errors = Self::default();
    errors.add_generic(value);
    errors
  }

  fn add_generic(&mut self, value: String) {
    self.generic.push(value);
  }

  fn add_specific(&mut self, key: String, value: Vec<String>) {
    self.specific.insert(key, value);
  }
}

impl IntoResponse for Errors {
  fn into_response(self) -> Response {
    Json(self).into_response()
  }
}

#[derive(Debug, Error)]
pub enum HandlerError {
  #[error("{0}.")]
  JsonRejection(#[from] JsonRejection),
  #[error("Some inputs failed validation.")]
  Validation(#[from] ValidationErrors),
}

impl HandlerError {
  fn as_generic(&self, code: StatusCode) -> (StatusCode, Errors) {
    (code, Errors::generic(self.to_string()))
  }
}

fn format_error_messages(field: &str, errors: &ValidationErrorsKind) -> Vec<String> {
  let title = field.to_case(Case::Title);
  match errors {
    ValidationErrorsKind::Field(errors) => errors
      .iter()
      .map(|e| match e.code.as_ref() {
        "email" => format!("{} must be a valid email address.", title),
        "must_match" => format!(
          "{} must be identical to {}.",
          title,
          e.message.as_ref().unwrap()
        ),
        "length" => format!(
          "{} must be between {} and {} characters long (currently {}).",
          title,
          e.params.get("min").unwrap(),
          e.params.get("max").unwrap(),
          e.params
            .get("value")
            .unwrap()
            .as_str()
            .unwrap()
            .chars()
            .count(),
        ),
        "regex" => format!("{} must {}.", title, e.message.as_ref().unwrap()),
        code => unimplemented!(
          "error message is not implemented for message code '{}'",
          code
        ),
      })
      .collect(),
    ValidationErrorsKind::Struct(_) | ValidationErrorsKind::List(_) => {
      panic!("unexpected error type")
    }
  }
}

impl IntoResponse for HandlerError {
  fn into_response(self) -> Response {
    match self {
      HandlerError::JsonRejection(_) => self.as_generic(StatusCode::BAD_REQUEST),
      HandlerError::Validation(ref validation_errors) => (StatusCode::BAD_REQUEST, {
        let mut errors = Errors::generic(self.to_string());
        for (k, v) in validation_errors.errors() {
          errors.add_specific(k.to_case(Case::Camel), format_error_messages(k, v));
        }
        errors
      }),
    }
    .into_response()
  }
}

pub struct ValidatedJson<T>(pub T);

impl<T, S> FromRequest<S> for ValidatedJson<T>
where
  T: DeserializeOwned + Validate,
  S: Send + Sync,
  Json<T>: FromRequest<S, Rejection = JsonRejection>,
{
  type Rejection = HandlerError;
  async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
    let Json(value) = Json::from_request(req, state).await?;
    value.validate()?;
    Ok(ValidatedJson(value))
  }
}
