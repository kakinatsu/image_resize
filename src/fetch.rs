use axum::{
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::task;
use tracing::{error, info};

use crate::{
    AppState,
    db::{self, ImageRecord},
    image_processing,
    r2::R2Error,
};

pub async fn get_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, StatusCode> {
    let requested_id = id;
    let db_path = state.sqlite_path.clone();
    let lookup_id = requested_id.clone();
    let image = task::spawn_blocking(move || db::find_image_by_id(&db_path, &lookup_id))
        .await
        .map_err(|err| {
            error!("database task failed: {err}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map_err(|err| {
            error!("{err}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !is_image_available(&image, OffsetDateTime::now_utc()).map_err(|err| {
        error!("failed to parse image expiry: {err}");
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        return Err(StatusCode::NOT_FOUND);
    }

    info!(
        "fetching image from r2: id={}, object_key={}",
        requested_id, image.object_key
    );
    let bytes = state
        .r2
        .get_object(&image.object_key)
        .await
        .map_err(|err| match err {
            R2Error::ObjectNotFound => {
                error!(
                    "image object was not found in r2: id={}, object_key={}",
                    requested_id, image.object_key
                );
                StatusCode::NOT_FOUND
            }
            other => {
                error!("{other}");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    Ok((
        [
            (header::CONTENT_TYPE, image_processing::OUTPUT_CONTENT_TYPE),
            (header::CACHE_CONTROL, "no-store"),
            (
                header::HeaderName::from_static("x-content-type-options"),
                "nosniff",
            ),
        ],
        bytes,
    )
        .into_response())
}

fn is_image_available(
    record: &ImageRecord,
    now: OffsetDateTime,
) -> Result<bool, time::error::Parse> {
    if record.deleted_at.is_some() {
        return Ok(false);
    }

    let expires_at = OffsetDateTime::parse(&record.expires_at, &Rfc3339)?;
    Ok(now < expires_at)
}

#[cfg(test)]
mod tests {
    use time::{Date, Month, PrimitiveDateTime, Time};

    use super::is_image_available;
    use crate::db::ImageRecord;

    #[test]
    fn deleted_image_is_not_available() {
        let record = sample_record(Some("2026-03-07T02:00:00Z"), "2026-03-07T03:00:00Z");
        let now = timestamp(2026, Month::March, 7, 1, 0, 0);

        assert!(!is_image_available(&record, now).unwrap());
    }

    #[test]
    fn expired_image_is_not_available() {
        let record = sample_record(None, "2026-03-07T03:00:00Z");
        let now = timestamp(2026, Month::March, 7, 3, 0, 0);

        assert!(!is_image_available(&record, now).unwrap());
    }

    #[test]
    fn active_image_is_available() {
        let record = sample_record(None, "2026-03-07T03:00:01Z");
        let now = timestamp(2026, Month::March, 7, 3, 0, 0);

        assert!(is_image_available(&record, now).unwrap());
    }

    fn sample_record(deleted_at: Option<&str>, expires_at: &str) -> ImageRecord {
        ImageRecord {
            id: "image-a".to_owned(),
            object_key: "images/2026/03/07/image-a.webp".to_owned(),
            content_type: "image/webp".to_owned(),
            width: 640,
            height: 480,
            size_bytes: 12345,
            created_at: "2026-03-07T00:00:00Z".to_owned(),
            expires_at: expires_at.to_owned(),
            deleted_at: deleted_at.map(str::to_owned),
        }
    }

    fn timestamp(
        year: i32,
        month: Month,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
    ) -> time::OffsetDateTime {
        let date = Date::from_calendar_date(year, month, day).unwrap();
        let time = Time::from_hms(hour, minute, second).unwrap();
        PrimitiveDateTime::new(date, time).assume_utc()
    }
}
