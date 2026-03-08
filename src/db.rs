use std::{fs, path::Path};

use rusqlite::{Connection, OptionalExtension, params};

const INIT_SQL: &str = include_str!("../sql/init.sql");

#[derive(Clone, Debug)]
pub struct NewImageRecord {
    pub id: String,
    pub object_key: String,
    pub content_type: String,
    pub width: i64,
    pub height: i64,
    pub size_bytes: i64,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageRecord {
    pub id: String,
    pub object_key: String,
    pub content_type: String,
    pub width: i64,
    pub height: i64,
    pub size_bytes: i64,
    pub created_at: String,
    pub expires_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CleanupCandidate {
    pub id: String,
    pub object_key: String,
}

pub fn initialize_database(path: &Path) -> Result<(), DbInitError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(DbInitError::CreateDirectory)?;
        }
    }

    let connection = Connection::open(path).map_err(DbInitError::OpenConnection)?;
    connection
        .execute_batch(INIT_SQL)
        .map_err(DbInitError::InitializeSchema)?;

    Ok(())
}

pub fn insert_image(path: &Path, record: &NewImageRecord) -> Result<(), DbWriteError> {
    let connection = Connection::open(path).map_err(DbWriteError::OpenConnection)?;
    connection
        .execute(
            "INSERT INTO images (id, object_key, content_type, width, height, size_bytes, created_at, expires_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)",
            params![
                record.id,
                record.object_key,
                record.content_type,
                record.width,
                record.height,
                record.size_bytes,
                record.created_at,
                record.expires_at,
            ],
        )
        .map_err(DbWriteError::InsertImage)?;

    Ok(())
}

pub fn find_image_by_id(path: &Path, id: &str) -> Result<Option<ImageRecord>, DbReadError> {
    let connection = Connection::open(path).map_err(DbReadError::OpenConnection)?;
    connection
        .query_row(
            "SELECT id, object_key, content_type, width, height, size_bytes, created_at, expires_at, deleted_at
             FROM images
             WHERE id = ?1",
            params![id],
            |row| {
                Ok(ImageRecord {
                    id: row.get(0)?,
                    object_key: row.get(1)?,
                    content_type: row.get(2)?,
                    width: row.get(3)?,
                    height: row.get(4)?,
                    size_bytes: row.get(5)?,
                    created_at: row.get(6)?,
                    expires_at: row.get(7)?,
                    deleted_at: row.get(8)?,
                })
            },
        )
        .optional()
        .map_err(DbReadError::QueryImage)
}

pub fn list_expired_images(
    path: &Path,
    cutoff: &str,
) -> Result<Vec<CleanupCandidate>, DbReadError> {
    let connection = Connection::open(path).map_err(DbReadError::OpenConnection)?;
    let mut statement = connection
        .prepare(
            "SELECT id, object_key
             FROM images
             WHERE expires_at < ?1
               AND deleted_at IS NULL",
        )
        .map_err(DbReadError::PrepareExpiredImages)?;

    let rows = statement
        .query_map(params![cutoff], |row| {
            Ok(CleanupCandidate {
                id: row.get(0)?,
                object_key: row.get(1)?,
            })
        })
        .map_err(DbReadError::QueryExpiredImages)?;

    let mut candidates = Vec::new();
    for row in rows {
        candidates.push(row.map_err(DbReadError::ReadExpiredImageRow)?);
    }

    Ok(candidates)
}

pub fn mark_image_deleted(path: &Path, id: &str, deleted_at: &str) -> Result<(), DbWriteError> {
    let connection = Connection::open(path).map_err(DbWriteError::OpenConnection)?;
    let changed = connection
        .execute(
            "UPDATE images
             SET deleted_at = ?1
             WHERE id = ?2",
            params![deleted_at, id],
        )
        .map_err(DbWriteError::MarkDeleted)?;

    if changed == 1 {
        Ok(())
    } else {
        Err(DbWriteError::ImageNotFound(id.to_owned()))
    }
}

#[derive(Debug)]
pub enum DbInitError {
    CreateDirectory(std::io::Error),
    OpenConnection(rusqlite::Error),
    InitializeSchema(rusqlite::Error),
}

impl std::fmt::Display for DbInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateDirectory(err) => write!(f, "failed to create database directory: {err}"),
            Self::OpenConnection(err) => write!(f, "failed to open sqlite database: {err}"),
            Self::InitializeSchema(err) => write!(f, "failed to initialize sqlite schema: {err}"),
        }
    }
}

impl std::error::Error for DbInitError {}

#[derive(Debug)]
pub enum DbReadError {
    OpenConnection(rusqlite::Error),
    QueryImage(rusqlite::Error),
    PrepareExpiredImages(rusqlite::Error),
    QueryExpiredImages(rusqlite::Error),
    ReadExpiredImageRow(rusqlite::Error),
}

impl std::fmt::Display for DbReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenConnection(err) => write!(f, "failed to open sqlite database: {err}"),
            Self::QueryImage(err) => write!(f, "failed to query image metadata: {err}"),
            Self::PrepareExpiredImages(err) => write!(f, "failed to prepare cleanup query: {err}"),
            Self::QueryExpiredImages(err) => write!(f, "failed to query expired images: {err}"),
            Self::ReadExpiredImageRow(err) => write!(f, "failed to read expired image row: {err}"),
        }
    }
}

impl std::error::Error for DbReadError {}

#[derive(Debug)]
pub enum DbWriteError {
    OpenConnection(rusqlite::Error),
    InsertImage(rusqlite::Error),
    MarkDeleted(rusqlite::Error),
    ImageNotFound(String),
}

impl std::fmt::Display for DbWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenConnection(err) => write!(f, "failed to open sqlite database: {err}"),
            Self::InsertImage(err) => write!(f, "failed to insert image metadata: {err}"),
            Self::MarkDeleted(err) => write!(f, "failed to update deleted_at: {err}"),
            Self::ImageNotFound(id) => {
                write!(f, "image {id} was not found while updating deleted_at")
            }
        }
    }
}

impl std::error::Error for DbWriteError {}

#[cfg(test)]
mod tests {
    use std::{env, fs, path::PathBuf};

    use ulid::Ulid;

    use super::{
        NewImageRecord, find_image_by_id, initialize_database, insert_image, list_expired_images,
        mark_image_deleted,
    };

    #[test]
    fn find_image_by_id_returns_saved_record() {
        let path = temp_db_path("find_image_by_id");
        initialize_database(&path).unwrap();
        insert_image(&path, &sample_record("image-a", "2026-03-07T03:00:00Z")).unwrap();

        let image = find_image_by_id(&path, "image-a").unwrap().unwrap();
        assert_eq!(image.id, "image-a");
        assert_eq!(image.object_key, "images/2026/03/07/image-a.webp");
        assert!(image.deleted_at.is_none());

        cleanup_db_file(&path);
    }

    #[test]
    fn list_expired_images_ignores_deleted_and_unexpired_rows() {
        let path = temp_db_path("list_expired_images");
        initialize_database(&path).unwrap();
        insert_image(&path, &sample_record("expired", "2026-03-07T01:00:00Z")).unwrap();
        insert_image(&path, &sample_record("active", "2026-03-07T15:00:00Z")).unwrap();
        insert_image(&path, &sample_record("deleted", "2026-03-07T00:30:00Z")).unwrap();
        mark_image_deleted(&path, "deleted", "2026-03-07T12:00:00Z").unwrap();

        let images = list_expired_images(&path, "2026-03-07T12:00:00Z").unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].id, "expired");

        cleanup_db_file(&path);
    }

    #[test]
    fn mark_image_deleted_updates_deleted_at() {
        let path = temp_db_path("mark_image_deleted");
        initialize_database(&path).unwrap();
        insert_image(&path, &sample_record("image-b", "2026-03-07T03:00:00Z")).unwrap();

        mark_image_deleted(&path, "image-b", "2026-03-07T12:00:00Z").unwrap();

        let image = find_image_by_id(&path, "image-b").unwrap().unwrap();
        assert_eq!(image.deleted_at.as_deref(), Some("2026-03-07T12:00:00Z"));

        cleanup_db_file(&path);
    }

    fn sample_record(id: &str, expires_at: &str) -> NewImageRecord {
        NewImageRecord {
            id: id.to_owned(),
            object_key: format!("images/2026/03/07/{id}.webp"),
            content_type: "image/webp".to_owned(),
            width: 640,
            height: 480,
            size_bytes: 12345,
            created_at: "2026-03-07T00:00:00Z".to_owned(),
            expires_at: expires_at.to_owned(),
        }
    }

    fn temp_db_path(test_name: &str) -> PathBuf {
        env::temp_dir().join(format!("image_resize_{test_name}_{}.db", Ulid::new()))
    }

    fn cleanup_db_file(path: &PathBuf) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(path.with_extension("db-shm"));
        let _ = fs::remove_file(path.with_extension("db-wal"));
    }
}
