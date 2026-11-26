use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use shared::ApiErrorResponse;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Unauthorized(String),

    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    NotFound(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            Self::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", msg.clone()),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg.clone()),
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone()),
            Self::Database(err) => {
                tracing::error!("SQLite error: {err}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DATABASE_ERROR",
                    "เกิดข้อผิดพลาดในการเชื่อมต่อฐานข้อมูล".to_string(),
                )
            }
            Self::Io(err) => {
                tracing::error!("I/O error: {err}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "IO_ERROR",
                    "เกิดข้อผิดพลาดในการจัดเก็บไฟล์".to_string(),
                )
            }
            Self::Internal(err) => {
                tracing::error!("Internal server error: {err}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "เกิดข้อผิดพลาดภายในระบบ".to_string(),
                )
            }
        };

        (
            status,
            Json(ApiErrorResponse {
                error: message,
                code: code.to_string(),
            }),
        )
            .into_response()
    }
}
