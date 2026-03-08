CREATE TABLE IF NOT EXISTS images (
    id           TEXT PRIMARY KEY,
    object_key   TEXT NOT NULL UNIQUE,
    content_type TEXT NOT NULL,
    width        INTEGER NOT NULL CHECK (width > 0),
    height       INTEGER NOT NULL CHECK (height > 0),
    size_bytes   INTEGER NOT NULL CHECK (size_bytes >= 0),
    created_at   TEXT NOT NULL,
    expires_at   TEXT NOT NULL,
    deleted_at   TEXT NULL
);

CREATE INDEX IF NOT EXISTS idx_images_expires_at
ON images (expires_at);

CREATE INDEX IF NOT EXISTS idx_images_deleted_at
ON images (deleted_at);
