-- Initial schema migration (v001)
-- This is the SQL representation of the schema for reference.

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

CREATE TABLE IF NOT EXISTS config (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_media_sort ON media_items(file_created_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_media_path ON media_items(relative_path);
CREATE INDEX IF NOT EXISTS idx_media_mime ON media_items(mime_type);
