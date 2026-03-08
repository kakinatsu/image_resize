use std::borrow::Cow;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: Cow<'static, str>,
}

impl ApiError {
    pub fn invalid_parameter(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "INVALID_PARAMETER", message)
    }

    pub fn missing_file() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "MISSING_FILE",
            "multipart field `file` is required",
        )
    }

    pub fn file_too_large() -> Self {
        Self::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "FILE_TOO_LARGE",
            "uploaded file exceeds the 10 MB limit",
        )
    }

    pub fn unsupported_media_type() -> Self {
        Self::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "UNSUPPORTED_MEDIA_TYPE",
            "supported image formats are JPEG, PNG, and WebP",
        )
    }

    pub fn invalid_image() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "INVALID_IMAGE",
            "failed to decode image data",
        )
    }

    pub fn internal_error() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "internal server error",
        )
    }

    fn new(status: StatusCode, code: &'static str, message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorResponse {
            error: ErrorBody {
                code: self.code,
                message: self.message.into_owned(),
            },
        };

        (self.status, Json(body)).into_response()
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}
