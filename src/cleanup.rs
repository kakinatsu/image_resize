use std::path::PathBuf;

use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tracing::{error, info};

use crate::{
    config::Config,
    db::{self, CleanupCandidate},
    r2::{R2Client, R2ConfigError, R2Error},
};

#[derive(Clone, Copy, Debug)]
pub struct CleanupReport {
    pub candidates: usize,
    pub deleted: usize,
    pub failed: usize,
}

pub async fn run_cleanup(config: &Config) -> Result<CleanupReport, CleanupError> {
    let now = OffsetDateTime::now_utc();
    let cutoff = format_rfc3339(now).map_err(CleanupError::FormatTimestamp)?;
    let candidates = db::list_expired_images(&config.sqlite_path, &cutoff)
        .map_err(CleanupError::ReadDatabase)?;

    if candidates.is_empty() {
        let report = CleanupReport {
            candidates: 0,
            deleted: 0,
            failed: 0,
        };
        info!("cleanup finished: candidates=0, deleted=0, failed=0");
        return Ok(report);
    }

    let r2 = R2Client::new(config).map_err(CleanupError::R2Config)?;
    let sqlite_path = config.sqlite_path.clone();
    let total = candidates.len();
    let mut deleted = 0;
    let mut failed = 0;

    for candidate in candidates {
        match delete_candidate(&sqlite_path, &r2, &candidate, now).await {
            Ok(()) => deleted += 1,
            Err(err) => {
                failed += 1;
                error!("cleanup failed for {}: {err}", candidate.id);
            }
        }
    }

    let report = CleanupReport {
        candidates: total,
        deleted,
        failed,
    };
    info!(
        "cleanup finished: candidates={}, deleted={}, failed={}",
        report.candidates, report.deleted, report.failed
    );

    if report.failed > 0 {
        Err(CleanupError::PartialFailure(report))
    } else {
        Ok(report)
    }
}

async fn delete_candidate(
    sqlite_path: &PathBuf,
    r2: &R2Client,
    candidate: &CleanupCandidate,
    deleted_at: OffsetDateTime,
) -> Result<(), CleanupItemError> {
    match r2.delete_object(&candidate.object_key).await {
        Ok(()) | Err(R2Error::ObjectNotFound) => {}
        Err(err) => return Err(CleanupItemError::R2(err)),
    }

    let deleted_at_text = format_rfc3339(deleted_at).map_err(CleanupItemError::FormatTimestamp)?;
    db::mark_image_deleted(sqlite_path, &candidate.id, &deleted_at_text)
        .map_err(CleanupItemError::WriteDatabase)
}

fn format_rfc3339(value: OffsetDateTime) -> Result<String, time::error::Format> {
    value.format(&Rfc3339)
}

#[derive(Debug)]
pub enum CleanupError {
    FormatTimestamp(time::error::Format),
    ReadDatabase(db::DbReadError),
    R2Config(R2ConfigError),
    PartialFailure(CleanupReport),
}

impl std::fmt::Display for CleanupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FormatTimestamp(err) => write!(f, "failed to format cleanup timestamp: {err}"),
            Self::ReadDatabase(err) => write!(f, "failed to load cleanup candidates: {err}"),
            Self::R2Config(err) => write!(f, "{err}"),
            Self::PartialFailure(report) => write!(
                f,
                "cleanup completed with failures: deleted={}, failed={}",
                report.deleted, report.failed
            ),
        }
    }
}

impl std::error::Error for CleanupError {}

#[derive(Debug)]
enum CleanupItemError {
    FormatTimestamp(time::error::Format),
    WriteDatabase(db::DbWriteError),
    R2(R2Error),
}

impl std::fmt::Display for CleanupItemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FormatTimestamp(err) => {
                write!(f, "failed to format deleted_at timestamp: {err}")
            }
            Self::WriteDatabase(err) => write!(f, "failed to update deleted_at: {err}"),
            Self::R2(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for CleanupItemError {}

#[cfg(test)]
mod tests {
    use std::{env, fs, path::PathBuf};

    use ulid::Ulid;

    use super::run_cleanup;
    use crate::{config::Config, db};

    #[tokio::test]
    async fn run_cleanup_returns_empty_report_when_there_are_no_candidates() {
        let sqlite_path = env::temp_dir().join(format!("image_resize_cleanup_{}.db", Ulid::new()));
        db::initialize_database(&sqlite_path).unwrap();

        let config = Config {
            app_addr: "127.0.0.1:3000".parse().unwrap(),
            public_base_url: "http://127.0.0.1:3000".to_owned(),
            sqlite_path: sqlite_path.clone(),
            static_dir: PathBuf::from("static"),
            r2_endpoint: "https://example-account.r2.cloudflarestorage.com".to_owned(),
            r2_bucket: "example-bucket".to_owned(),
            r2_access_key_id: "example-access-key".to_owned(),
            r2_secret_access_key: "example-secret-key".to_owned(),
            r2_region: "auto".to_owned(),
        };

        let report = run_cleanup(&config).await.unwrap();
        assert_eq!(report.candidates, 0);
        assert_eq!(report.deleted, 0);
        assert_eq!(report.failed, 0);

        let _ = fs::remove_file(sqlite_path);
    }
}
