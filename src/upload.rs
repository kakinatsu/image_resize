use axum::{
    Json,
    extract::{
        DefaultBodyLimit, Multipart, Query, State, multipart::MultipartError,
        rejection::QueryRejection,
    },
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::task;
use tracing::{error, info};
use ulid::Ulid;

use crate::{
    AppState,
    db::{self, NewImageRecord},
    error::ApiError,
    image_processing::{self, ImageProcessingError},
};

pub const MAX_FILE_BYTES: usize = 10_000_000;
pub const MAX_MULTIPART_BODY_BYTES: usize = 12_000_000;
const DEFAULT_MAX_DIMENSION: u32 = 2048;
const MAX_ALLOWED_DIMENSION: u32 = 4096;
const EXPIRATION_HOURS: i64 = 12;

#[derive(Debug, Deserialize)]
pub struct UploadQuery {
    max_width: Option<u32>,
    max_height: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct UploadImageResponse {
    id: String,
    url: String,
    expires_at: String,
    width: u32,
    height: u32,
    content_type: &'static str,
    size_bytes: u64,
}

pub async fn upload_image(
    State(state): State<AppState>,
    query: Result<Query<UploadQuery>, QueryRejection>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    let options = resolve_query(query)?;
    let file_bytes = read_uploaded_file(&mut multipart).await?;

    let processed = task::spawn_blocking({
        let file_bytes = file_bytes.clone();
        move || image_processing::process_upload(&file_bytes, options.max_width, options.max_height)
    })
    .await
    .map_err(|err| {
        error!("image processing task failed: {err}");
        ApiError::internal_error()
    })?
    .map_err(map_image_processing_error)?;

    let now = OffsetDateTime::now_utc();
    let expires_at = now + Duration::hours(EXPIRATION_HOURS);
    let id = Ulid::new().to_string();
    let object_key = build_object_key(&id, now);

    state
        .r2
        .put_object(
            &object_key,
            processed.bytes.clone(),
            image_processing::OUTPUT_CONTENT_TYPE,
        )
        .await
        .map_err(|err| {
            error!("{err}");
            ApiError::internal_error()
        })?;

    let created_at_text = format_rfc3339(now)?;
    let expires_at_text = format_rfc3339(expires_at)?;
    let record = NewImageRecord {
        id: id.clone(),
        object_key: object_key.clone(),
        content_type: image_processing::OUTPUT_CONTENT_TYPE.to_owned(),
        width: i64::from(processed.width),
        height: i64::from(processed.height),
        size_bytes: processed.bytes.len() as i64,
        created_at: created_at_text,
        expires_at: expires_at_text.clone(),
    };

    let db_path = state.sqlite_path.clone();
    task::spawn_blocking(move || db::insert_image(&db_path, &record))
        .await
        .map_err(|err| {
            error!("database task failed: {err}");
            ApiError::internal_error()
        })?
        .map_err(|err| {
            error!("{err}");
            ApiError::internal_error()
        })?;

    info!(
        "stored image metadata: id={}, object_key={}",
        id, object_key
    );

    let response = UploadImageResponse {
        id: id.clone(),
        url: image_url(&state.public_base_url, &id),
        expires_at: expires_at_text,
        width: processed.width,
        height: processed.height,
        content_type: image_processing::OUTPUT_CONTENT_TYPE,
        size_bytes: processed.bytes.len() as u64,
    };

    Ok((StatusCode::CREATED, Json(response)))
}

fn resolve_query(
    query: Result<Query<UploadQuery>, QueryRejection>,
) -> Result<ResolvedUploadQuery, ApiError> {
    let Query(query) = query.map_err(|_| {
        ApiError::invalid_parameter("max_width and max_height must be integers between 1 and 4096")
    })?;

    let max_width = query.max_width.unwrap_or(DEFAULT_MAX_DIMENSION);
    let max_height = query.max_height.unwrap_or(DEFAULT_MAX_DIMENSION);

    if !(1..=MAX_ALLOWED_DIMENSION).contains(&max_width)
        || !(1..=MAX_ALLOWED_DIMENSION).contains(&max_height)
    {
        return Err(ApiError::invalid_parameter(
            "max_width and max_height must be between 1 and 4096",
        ));
    }

    Ok(ResolvedUploadQuery {
        max_width,
        max_height,
    })
}

async fn read_uploaded_file(multipart: &mut Multipart) -> Result<Vec<u8>, ApiError> {
    while let Some(field) = multipart.next_field().await.map_err(map_multipart_error)? {
        if field.name() != Some("file") {
            continue;
        }

        let bytes = field.bytes().await.map_err(map_multipart_error)?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(ApiError::file_too_large());
        }

        return Ok(bytes.to_vec());
    }

    Err(ApiError::missing_file())
}

fn map_multipart_error(err: MultipartError) -> ApiError {
    match err.status() {
        StatusCode::PAYLOAD_TOO_LARGE => ApiError::file_too_large(),
        StatusCode::BAD_REQUEST => {
            ApiError::invalid_parameter("invalid multipart/form-data request")
        }
        _ => {
            error!("multipart handling failed: {err}");
            ApiError::internal_error()
        }
    }
}

fn map_image_processing_error(err: ImageProcessingError) -> ApiError {
    match err {
        ImageProcessingError::UnsupportedFormat => ApiError::unsupported_media_type(),
        ImageProcessingError::InvalidImage => ApiError::invalid_image(),
        ImageProcessingError::EncodeWebp => {
            error!("failed to encode processed image as webp");
            ApiError::internal_error()
        }
    }
}

fn format_rfc3339(value: OffsetDateTime) -> Result<String, ApiError> {
    value.format(&Rfc3339).map_err(|err| {
        error!("failed to format timestamp: {err}");
        ApiError::internal_error()
    })
}

fn build_object_key(id: &str, timestamp: OffsetDateTime) -> String {
    format!(
        "images/{:04}/{:02}/{:02}/{id}.webp",
        timestamp.year(),
        u8::from(timestamp.month()),
        timestamp.day(),
    )
}

fn image_url(base_url: &str, id: &str) -> String {
    format!("{}/i/{id}", base_url.trim_end_matches('/'))
}

#[derive(Debug)]
struct ResolvedUploadQuery {
    max_width: u32,
    max_height: u32,
}

pub fn upload_body_limit() -> DefaultBodyLimit {
    DefaultBodyLimit::max(MAX_MULTIPART_BODY_BYTES)
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use time::{Date, Month, PrimitiveDateTime, Time};

    use super::{
        DEFAULT_MAX_DIMENSION, MAX_ALLOWED_DIMENSION, UploadQuery, build_object_key, image_url,
        resolve_query,
    };

    #[test]
    fn resolve_query_uses_defaults() {
        let resolved = resolve_query(Ok(Query(UploadQuery {
            max_width: None,
            max_height: None,
        })))
        .unwrap();

        assert_eq!(resolved.max_width, DEFAULT_MAX_DIMENSION);
        assert_eq!(resolved.max_height, DEFAULT_MAX_DIMENSION);
    }

    #[test]
    fn resolve_query_rejects_out_of_range_values() {
        let result = resolve_query(Ok(Query(UploadQuery {
            max_width: Some(MAX_ALLOWED_DIMENSION + 1),
            max_height: Some(100),
        })));

        assert!(result.is_err());
    }

    #[test]
    fn build_object_key_uses_expected_layout() {
        let date = Date::from_calendar_date(2026, Month::March, 7).unwrap();
        let time = Time::from_hms(12, 30, 0).unwrap();
        let timestamp = PrimitiveDateTime::new(date, time).assume_utc();

        let key = build_object_key("01JXYZABCDEF1234567890ABCD", timestamp);
        assert_eq!(key, "images/2026/03/07/01JXYZABCDEF1234567890ABCD.webp");
    }

    #[test]
    fn image_url_trims_trailing_slash() {
        assert_eq!(
            image_url("https://example.com/", "01JXYZABCDEF1234567890ABCD"),
            "https://example.com/i/01JXYZABCDEF1234567890ABCD"
        );
    }
}
