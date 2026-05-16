mod schema;
pub mod indexer;

pub use indexer::ReindexStats;
pub use schema::build_schema;

use std::path::Path;
use std::sync::Mutex;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};
use tantivy::schema::Schema;

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
    writer: Mutex<Option<IndexWriter>>,
}

impl IndexManager {
    /// Open an existing Tantivy index at `path`, or create a new one.
    ///
    /// If the directory does not exist it is created.  If a valid Tantivy index
    /// is already present (detected by `meta.json`) it is opened; otherwise a
    /// fresh index is initialised.
    pub fn open_or_create(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let schema = build_schema();

        // Ensure the directory exists before opening the index.
        std::fs::create_dir_all(path)?;

        let index = if path.join("meta.json").exists() {
            Index::open_in_dir(path)?
        } else {
            Index::create_in_dir(path, schema.clone())?
        };

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        let writer = Some(index.writer(50_000_000)?);

        Ok(Self {
            index,
            schema,
            reader,
            writer: Mutex::new(writer),
        })
    }

    /// Add a document to the index.
    ///
    /// The document is buffered in memory until [`commit`](Self::commit)
    /// is called.
    pub fn add_document(&self, doc: TantivyDocument) -> Result<(), Box<dyn std::error::Error>> {
        let mut guard = self.writer.lock().unwrap();
        let writer = guard.as_mut().ok_or("IndexWriter has been consumed")?;
        writer.add_document(doc)?;
        Ok(())
    }

    /// Commit pending writes and reload the reader so searches see the new
    /// documents immediately.
    pub fn commit(&self) -> Result<(), Box<dyn std::error::Error>> {
        {
            let mut guard = self.writer.lock().unwrap();
            if let Some(writer) = guard.as_mut() {
                writer.commit()?;
            }
        }
        self.reader.reload()?;
        Ok(())
    }

    /// Convenience alias for [`commit`](Self::commit).
    pub fn refresh(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.commit()
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
        let mut guard = self.writer.lock().unwrap();
        let writer = guard.as_mut().ok_or("IndexWriter has been consumed")?;
        writer.delete_all_documents()?;
        Ok(())
    }

    /// Delete the document identified by a text field value.
    ///
    /// Used primarily by [`indexer::incremental_index`] to remove stale documents
    /// for re-indexed rows.  The field should be `STRING`-indexed for this to work
    /// predictably — in practice the `id` field is always used.
    pub fn delete_document_by_field(
        &self,
        field_name: &str,
        value: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let field = self.schema.get_field(field_name)?;
        let term = Term::from_field_text(field, value);
        let mut guard = self.writer.lock().unwrap();
        let writer = guard.as_mut().ok_or("IndexWriter has been consumed")?;
        writer.delete_term(term);
        Ok(())
    }
}
