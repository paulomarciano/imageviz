pub mod indexer;
mod schema;

pub use indexer::ReindexStats;
pub use schema::build_schema;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use tantivy::schema::Schema;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

/// Schema version — bump this when the Tantivy schema changes.
/// Old indices with a different version are deleted and rebuilt from SQLite.
const SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// IndexManager – public API
// ---------------------------------------------------------------------------

/// Thread-safe manager for a Tantivy full-text search index.
///
/// The `IndexWriter` is wrapped in a `Mutex` so that the manager is `Send`
/// and `Sync`.  Callers that need to avoid blocking the async runtime should
/// wrap operations in `tokio::task::spawn_blocking`.
pub struct IndexManager {
    index: Index,
    schema: Schema,
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
}

impl IndexManager {
    /// Open an existing Tantivy index at `path`, or create a new one.
    ///
    /// If the directory does not exist it is created.  If a valid Tantivy index
    /// is already present (detected by `meta.json`) it is opened; otherwise a
    /// fresh index is initialised.
    ///
    /// A `.schema_version` file is maintained alongside the index.  When the
    /// schema version changes, the old index directory is removed and a fresh
    /// index is created.  The data is rebuilt from SQLite on the next full
    /// reindex.
    ///
    /// `writer_memory` controls the Tantivy writer memory budget in bytes.
    /// Use larger values (e.g. 200 MB) during full reindex for fewer segments
    /// and faster indexing, and smaller values (e.g. 50 MB) during incremental
    /// operation for lower memory footprint.
    pub fn open_or_create(
        path: &Path,
        writer_memory: usize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let schema = build_schema();

        // Ensure the directory exists before opening the index.
        std::fs::create_dir_all(path)?;

        let version_file = path.join(".schema_version");
        let needs_recreate = Self::read_schema_version(&version_file) != Some(SCHEMA_VERSION);

        if needs_recreate {
            // Remove the old index (if any) — it will be rebuilt from SQLite.
            if path.join("meta.json").exists() {
                tracing::info!("Tantivy schema version changed → removing old index at {:?}", path);
                std::fs::remove_dir_all(path)?;
                std::fs::create_dir_all(path)?;
            }
            Self::write_schema_version(&version_file, SCHEMA_VERSION)?;
        }

        let index = if path.join("meta.json").exists() {
            Index::open_in_dir(path)?
        } else {
            Index::create_in_dir(path, schema.clone())?
        };

        let reader =
            index.reader_builder().reload_policy(ReloadPolicy::OnCommitWithDelay).try_into()?;

        let writer = index.writer(writer_memory)?;

        Ok(Self { index, schema, reader, writer: Mutex::new(writer) })
    }

    /// Lock the writer, mapping a poisoned mutex onto the error type.
    fn writer_lock(&self) -> Result<MutexGuard<'_, IndexWriter>, Box<dyn std::error::Error>> {
        self.writer
            .lock()
            .map_err(|e| Box::new(std::io::Error::other(format!("Mutex poisoned: {}", e))) as _)
    }

    fn read_schema_version(path: &Path) -> Option<u32> {
        std::fs::read_to_string(path).ok().and_then(|s| s.trim().parse().ok())
    }

    fn write_schema_version(path: &Path, version: u32) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::write(path, version.to_string())?;
        Ok(())
    }

    /// Add a document to the index.
    ///
    /// The document is buffered in memory until [`commit`](Self::commit)
    /// is called.
    pub fn add_document(&self, doc: TantivyDocument) -> Result<(), Box<dyn std::error::Error>> {
        self.writer_lock()?.add_document(doc)?;
        Ok(())
    }

    /// Commit pending writes and reload the reader so searches see the new
    /// documents immediately.
    pub fn commit(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.writer_lock()?.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Borrow the index reader for executing search queries.
    pub fn reader(&self) -> &IndexReader {
        &self.reader
    }

    /// Borrow the index schema.
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Borrow the underlying index (e.g. for building a `QueryParser`).
    pub fn index(&self) -> &Index {
        &self.index
    }

    /// Remove every document from the index.
    pub fn delete_all_documents(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.writer_lock()?.delete_all_documents()?;
        Ok(())
    }

    /// Delete the document identified by a text field value.
    ///
    /// Used by the watcher pipeline to remove stale documents when a file
    /// is deleted or re-indexed.  The field should be `STRING`-indexed for
    /// this to work predictably — in practice the `id` field is always used.
    pub fn delete_document_by_field(
        &self,
        field_name: &str,
        value: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let field = self.schema.get_field(field_name)?;
        let term = Term::from_field_text(field, value);
        self.writer_lock()?.delete_term(term);
        Ok(())
    }
}
