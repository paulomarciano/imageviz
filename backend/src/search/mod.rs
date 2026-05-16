mod schema;
pub use schema::build_schema;

use std::path::Path;
use std::sync::mpsc;
use std::thread;
use tantivy::{Index, IndexReader, ReloadPolicy, TantivyDocument};
use tantivy::schema::Schema;

// ---------------------------------------------------------------------------
// Writer operations — sent over a channel to a dedicated background thread.
// ---------------------------------------------------------------------------

/// Operations that can be submitted to the background writer thread.
#[derive(Debug)]
pub enum WriteOp {
    /// Index a new document.
    AddDocument(TantivyDocument),
    /// Commit all pending writes to the index.
    Commit,
    /// Remove every document from the index.
    DeleteAll,
}

// ---------------------------------------------------------------------------
// Background writer thread — RAII guard
// ---------------------------------------------------------------------------

/// Owns the background thread that holds the [`IndexWriter`].
///
/// Dropping this handle closes the write channel (signalling the thread to
/// exit) and then joins the thread, ensuring the writer releases its file
/// locks before the directory is used again.
struct WriterThread {
    tx: mpsc::Sender<WriteOp>,
    handle: Option<thread::JoinHandle<()>>,
}

impl WriterThread {
    fn spawn(index: Index, ram_buffer_size: usize) -> Self {
        let (tx, rx) = mpsc::channel::<WriteOp>();

        let handle = thread::spawn(move || {
            let mut writer = match index.writer(ram_buffer_size) {
                Ok(w) => w,
                Err(e) => {
                    // Writer creation failure is unrecoverable here; the
                    // channel receiver is dropped so senders will observe
                    // a broken pipe on their next send.
                    eprintln!("IndexManager: failed to create IndexWriter: {e}");
                    return;
                }
            };

            while let Ok(op) = rx.recv() {
                let result = match op {
                    WriteOp::AddDocument(doc) => writer
                        .add_document(doc)
                        .map(|_| ())
                        .map_err(|e| format!("add_document: {e}")),
                    WriteOp::Commit => writer
                        .commit()
                        .map(|_| ())
                        .map_err(|e| format!("commit: {e}")),
                    WriteOp::DeleteAll => writer
                        .delete_all_documents()
                        .map(|_| ())
                        .map_err(|e| format!("delete_all_documents: {e}")),
                };

                if let Err(msg) = result {
                    eprintln!("IndexManager: {msg}");
                }
            }

            // Final commit before the thread exits so writes are durable.
            if let Err(e) = writer.commit() {
                eprintln!("IndexManager: final commit on shutdown failed: {e}");
            }
        });

        Self { tx, handle: Some(handle) }
    }

    fn send(&self, op: WriteOp) -> Result<(), Box<dyn std::error::Error>> {
        self.tx.send(op)?;
        Ok(())
    }

    fn clone_sender(&self) -> mpsc::Sender<WriteOp> {
        self.tx.clone()
    }
}

// Fields are dropped in declaration order by the compiler, but we need
// explicit control: close the write channel *before* joining the thread so
// the writer loop exits cleanly and the index lock is released.
impl Drop for WriterThread {
    fn drop(&mut self) {
        // Replace the sender with a dummy so we can move the real one out
        // and drop it, closing the channel and waking the writer thread.
        let real_tx = std::mem::replace(
            &mut self.tx,
            mpsc::channel::<WriteOp>().0,
        );
        drop(real_tx); // ← closes the channel

        // Now the thread should finish its `recv()` loop and exit.
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

// ---------------------------------------------------------------------------
// IndexManager – public API
// ---------------------------------------------------------------------------

/// Thread-safe manager for a Tantivy full-text search index.
///
/// Writes are dispatched to a background thread so the manager is usable from
/// async contexts without blocking the runtime on CPU‑bound indexing work.
///
/// The write handle obtained from [`writer()`](Self::writer) is `Clone`, making it
/// safe to share across threads for concurrent document ingestion.
pub struct IndexManager {
    index: Index,
    schema: Schema,
    reader: IndexReader,
    writer: WriterThread,
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

        let writer = WriterThread::spawn(index.clone(), 50_000_000);

        Ok(Self { index, schema, reader, writer })
    }

    /// Queue a document to be added to the index.
    ///
    /// The document is sent to the background writer thread and will be
    /// indexed when the writer processes it.  Call [`commit`](Self::commit) to
    /// make it visible to searches.
    pub fn add_document(&self, doc: TantivyDocument) -> Result<(), Box<dyn std::error::Error>> {
        self.writer.send(WriteOp::AddDocument(doc))
    }

    /// Commit pending writes and reload the reader.
    ///
    /// After this call the committed documents are visible to subsequent
    /// searches through this manager's reader.
    pub fn commit(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.writer.send(WriteOp::Commit)?;
        // Force a reload so the current reader sees the new segment.
        self.reader.reload()?;
        Ok(())
    }

    /// Convenience alias for [`commit`](Self::commit).
    ///
    /// Commits pending writes and reloads the reader.
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

    /// Return a clone-safe handle for submitting write operations.
    ///
    /// The returned [`Sender`](mpsc::Sender) is `Clone` and can be shared
    /// across threads, enabling concurrent document ingestion without
    /// synchronising through the `IndexManager` itself.
    pub fn writer(&self) -> mpsc::Sender<WriteOp> {
        self.writer.clone_sender()
    }

    /// Remove every document from the index.
    pub fn delete_all_documents(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.writer.send(WriteOp::DeleteAll)
    }
}
