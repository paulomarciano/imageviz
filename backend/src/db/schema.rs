/// Create the `media_items` table — the primary table for indexed media files.
pub const CREATE_MEDIA_ITEMS: &str = "
    CREATE TABLE IF NOT EXISTS media_items (
        id TEXT PRIMARY KEY NOT NULL,
        filename TEXT NOT NULL,
        relative_path TEXT NOT NULL UNIQUE,
        mime_type TEXT NOT NULL,
        width INTEGER,
        height INTEGER,
        file_size INTEGER NOT NULL,
        thumbnail_path TEXT,
        file_created_at TEXT NOT NULL,
        file_modified_at TEXT NOT NULL,
        indexed_at TEXT NOT NULL DEFAULT (datetime('now')),
        metadata_json TEXT,
        checksum TEXT
    );
";

/// Create the `config` table for application-level key-value configuration.
pub const CREATE_CONFIG_TABLE: &str = "
    CREATE TABLE IF NOT EXISTS config (
        key TEXT PRIMARY KEY NOT NULL,
        value TEXT NOT NULL
    );
";

/// Index for cursor-based pagination sorted by creation date descending (with id tiebreaker).
pub const CREATE_IDX_MEDIA_SORT: &str = "
    CREATE INDEX IF NOT EXISTS idx_media_sort
    ON media_items(file_created_at DESC, id);
";

/// Index for fast path-based lookups.
pub const CREATE_IDX_MEDIA_PATH: &str = "
    CREATE INDEX IF NOT EXISTS idx_media_path
    ON media_items(relative_path);
";

/// Index for filtering by mime type.
pub const CREATE_IDX_MEDIA_MIME: &str = "
    CREATE INDEX IF NOT EXISTS idx_media_mime
    ON media_items(mime_type);
";
