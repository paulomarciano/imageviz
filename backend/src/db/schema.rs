/// Create the `media_items` table — the primary table for indexed media files.
///
/// Version 1 schema: `relative_path` has a UNIQUE constraint.
/// Version 2 (migration v002): adds `folder_id` column and changes the
/// unique constraint to `(folder_id, relative_path)`.
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

/// Index for fast path-based lookups (v1).
pub const CREATE_IDX_MEDIA_PATH: &str = "
    CREATE INDEX IF NOT EXISTS idx_media_path
    ON media_items(relative_path);
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

/// Compound unique index on `(folder_id, relative_path)` — allows the same
/// relative path in different watched folders while preventing duplicates
/// within a single folder.
pub const CREATE_IDX_MEDIA_FOLDER_PATH: &str = "
    CREATE UNIQUE INDEX IF NOT EXISTS idx_media_folder_path
    ON media_items(folder_id, relative_path);
";

/// Index for filtering by mime type.
pub const CREATE_IDX_MEDIA_MIME: &str = "
    CREATE INDEX IF NOT EXISTS idx_media_mime
    ON media_items(mime_type);
";

/// SQL for migration v002: add watched_folders table, add folder_id column,
/// drop old path index, create compound unique index.
pub const MIGRATION_V002: &str = "
    CREATE TABLE IF NOT EXISTS watched_folders (
        id TEXT PRIMARY KEY NOT NULL,
        path TEXT NOT NULL UNIQUE,
        label TEXT
    );
    ALTER TABLE media_items ADD COLUMN folder_id TEXT REFERENCES watched_folders(id);
    DROP INDEX IF EXISTS idx_media_path;
    CREATE UNIQUE INDEX IF NOT EXISTS idx_media_folder_path
        ON media_items(folder_id, relative_path);
";
