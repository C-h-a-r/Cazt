use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// Stable error codes returned to the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidInput,
    RateLimited,
    DeviceNotFound,
    DeviceOffline,
    UnsupportedControl,
    MediaNotFound,
    MediaUnavailable,
    UnsupportedSource,
    CinejoyUnavailable,
    ProxyDenied,
    SsrfBlocked,
    DiscoveryFailed,
    CastTimeout,
    CastFailed,
    DlnaRejected,
    StatusUnavailable,
    HlsFailed,
    NetworkUnreachable,
    Internal,
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub status: StatusCode,
}

impl AppError {
    pub fn new(status: StatusCode, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            status,
        }
    }

    pub fn bad_request(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }

    pub fn not_found(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message)
    }

    pub fn conflict(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    pub fn timeout(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(StatusCode::GATEWAY_TIMEOUT, code, message)
    }

    pub fn unavailable(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, code, message)
    }

    pub fn forbidden(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Internal,
            message,
        )
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    code: ErrorCode,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status;
        let body = Json(ErrorBody {
            error: ErrorDetail {
                code: self.code,
                message: self.message,
            },
        });
        (status, body).into_response()
    }
}

impl From<oxicast::Error> for AppError {
    fn from(err: oxicast::Error) -> Self {
        let message = err.to_string();
        if message.to_ascii_lowercase().contains("timeout") {
            AppError::timeout(
                ErrorCode::CastTimeout,
                "The Cast device did not respond in time.",
            )
        } else {
            AppError::unavailable(ErrorCode::CastFailed, format!("Cast failed: {message}"))
        }
    }
}

impl From<rupnp::Error> for AppError {
    fn from(err: rupnp::Error) -> Self {
        AppError::unavailable(
            ErrorCode::DlnaRejected,
            format!("The DLNA device rejected the request: {err}"),
        )
    }
}

impl From<url::ParseError> for AppError {
    fn from(_: url::ParseError) -> Self {
        AppError::bad_request(ErrorCode::InvalidInput, "That URL is not valid.")
    }
}
