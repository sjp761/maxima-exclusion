use actix_web::{error, http::header::ContentType, HttpResponse};
use derive_more::{Display, Error};
use reqwest::StatusCode;

#[derive(Debug, Display, Error)]
pub enum ServiceError {
    #[display(fmt = "internal error")]
    InternalError,
}

impl error::ResponseError for ServiceError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code())
            .insert_header(ContentType::html())
            .body(self.to_string())
    }

    fn status_code(&self) -> StatusCode {
        match *self {
            ServiceError::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<std::io::Error> for ServiceError {
    fn from(_: std::io::Error) -> Self {
        ServiceError::InternalError
    }
}

impl From<serde_json::Error> for ServiceError {
    fn from(_: serde_json::Error) -> Self {
        ServiceError::InternalError
    }
}

impl From<anyhow::Error> for ServiceError {
    fn from(_: anyhow::Error) -> Self {
        ServiceError::InternalError
    }
}
