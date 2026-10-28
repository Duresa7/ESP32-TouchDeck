use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub trait ApiError: std::fmt::Display + Send {
    fn status(&self) -> StatusCode;
    fn body(&self) -> ErrorBody;
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}
#[derive(Debug, Clone)]
pub struct CommonError {
    pub status: StatusCode,
    pub body: ErrorBody,
}
impl CommonError {
    pub fn new(status: u16, code: &str, message: &str) -> Self {
        Self {
            status: StatusCode::from_u16(status).unwrap(),
            body: ErrorBody {
                code: code.into(),
                message: message.into(),
                details: None,
            },
        }
    }
    pub fn invalid(part: &str, reason: &str) -> Self {
        let mut e = Self::new(400, "request.invalid", "The request is invalid.");
        e.body.details = Some(serde_json::json!({"part": part, "reason": reason}));
        e
    }
}
impl std::fmt::Display for CommonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.body.message)
    }
}
impl std::error::Error for CommonError {}
impl ApiError for CommonError {
    fn status(&self) -> StatusCode {
        self.status
    }
    fn body(&self) -> ErrorBody {
        self.body.clone()
    }
}
impl IntoResponse for CommonError {
    fn into_response(self) -> Response {
        (self.status, Json(serde_json::json!({"error": self.body}))).into_response()
    }
}
