# Wave 3.1 — Set Up Tantivy Index Schema and Writer

| Field | Value |
|-------|-------|
| **Wave** | 3 — Backend: Search, Cursor Pagination & Real-time SSE |
| **Seq** | 01 |
| **Estimate** | 2 hours |
| **Depends on** | None (independent — needs Tantivy in Cargo.toml) |
| **Parallel** | No (foundation for all Wave 3 search tasks) |

---

## Overview

Set up the Tantivy full-text search engine. Define the index schema matching §4 of the development plan, create the index directory, and implement writer/reader management. Tantivy provides 10-100x faster full-text search compared to SQL `LIKE '%term%'`.

## Prerequisites

- `tantivy = "0.26"` in Cargo.toml
- Understanding of inverted indexes and full-text search concepts

## Reference Files

- `documents/plans/development-plan.md` — §4 Tantivy Index Schema (lines 285–295), §2 Tech Stack (Tantivy rationale)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/search/
├── mod.rs                       # Public interface (IndexManager, open/close)
├── schema.rs                    # Tantivy schema definition
└── schema_test.rs               # Co-located tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Tantivy schema defined with all fields from §4:
  - `id` — text (STRING | STORED)
  - `filename` — text (STRING | STORED)
  - `mime_type` — text (STRING)
  - `metadata_json` — text (TEXT) — indexed for full-text search
  - `created_at` — date (INDEXED)
  - `file_size` — u64 (INDEXED)
  - `width` — u64 (STORED)
  - `height` — u64 (STORED)
- [ ] Index can be created (on disk in a configurable directory)
- [ ] Documents can be added to the index (single + batch)
- [ ] Writer commits are persisted and readable by a reader
- [ ] Multiple readers are supported (concurrent reads)
- [ ] Index is opened lazily (doesn't block server startup)
- [ ] Unit test: create index, add document, search, verify found

## Implementation Notes

**Tantivy schema definition:**
```rust
use tantivy::schema::*;
use tantivy::{Index, doc};

pub fn build_schema() -> Schema {
    let mut schema_builder = Schema::builder();
    
    schema_builder.add_text_field("id", STRING | STORED);
    schema_builder.add_text_field("filename", STRING | STORED);
    schema_builder.add_text_field("mime_type", STRING);
    schema_builder.add_text_field("metadata_json", TEXT);  // Full-text searchable
    schema_builder.add_date_field("created_at", INDEXED);
    schema_builder.add_u64_field("file_size", INDEXED);
    schema_builder.add_u64_field("width", STORED);
    schema_builder.add_u64_field("height", STORED);
    
    schema_builder.build()
}
```

**IndexManager struct:**
```rust
use tantivy::{Index, IndexWriter, IndexReader, ReloadPolicy};
use tantivy::directory::MmapDirectory;
use std::path::PathBuf;
use std::sync::Arc;

pub struct IndexManager {
    index: Index,
    schema: Schema,
    writer: Arc<Mutex<Option<IndexWriter>>>,
    reader: IndexReader,
}

impl IndexManager {
    pub fn open(path: &Path) -> Result<Self, tantivy::TantivyError> {
        let schema = build_schema();
        let dir = MmapDirectory::open(path)?;
        let index = Index::open_or_create(dir, schema.clone())?;
        
        let writer = index.writer(50_000_000)?; // 50MB buffer
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        
        Ok(Self {
            index,
            schema,
            writer: Arc::new(Mutex::new(Some(writer))),
            reader,
        })
    }
    
    pub fn add_document(&self, doc_data: IndexableDoc) -> Result<(), Error> {
        let schema = &self.schema;
        let doc = doc!(
            schema.get_field("id").unwrap() => doc_data.id,
            schema.get_field("filename").unwrap() => doc_data.filename,
            schema.get_field("mime_type").unwrap() => doc_data.mime_type,
            schema.get_field("metadata_json").unwrap() => doc_data.metadata_json,
            schema.get_field("created_at").unwrap() => doc_data.created_at,
            schema.get_field("file_size").unwrap() => doc_data.file_size,
            schema.get_field("width").unwrap() => doc_data.width,
            schema.get_field("height").unwrap() => doc_data.height,
        );
        
        let mut writer = self.writer.lock().unwrap();
        writer.as_mut().ok_or(Error::WriterClosed)?.add_document(doc)?;
        Ok(())
    }
    
    pub fn commit(&self) -> Result<u64, Error> {
        let mut writer = self.writer.lock().unwrap();
        writer.as_mut().ok_or(Error::WriterClosed)?.commit()?;
        Ok(0) // Ops count
    }
    
    pub fn reader(&self) -> &IndexReader {
        &self.reader
    }
    
    pub fn schema(&self) -> &Schema {
        &self.schema
    }
}
```

**Storage** — Use `MmapDirectory` (memory-mapped files) for performance. The index is stored on disk at a configurable path (default: `./data/tantivy/`).

**Field types explanation:**
- `STRING` — exact match, not tokenized (good for IDs, filenames)
- `TEXT` — tokenized, stemmed, full-text searchable (good for metadata_json content)
- `STORED` — can retrieve original value in search results
- `INDEXED` — can search/filter by this field

## Test Strategy

```rust
#[test]
fn test_create_index_and_search() {
    let dir = tempfile::tempdir().unwrap();
    let manager = IndexManager::open(dir.path()).unwrap();
    
    // Add document
    manager.add_document(IndexableDoc {
        id: "test-1".into(),
        filename: "image.png".into(),
        mime_type: "image/png".into(),
        metadata_json: r#"{"prompt": "a beautiful sunset"}"#.into(),
        created_at: tantivy::DateTime::from_utc(2025, 1, 1),
        file_size: 1024,
        width: 800,
        height: 600,
    }).unwrap();
    manager.commit().unwrap();
    
    // Search
    let reader = manager.reader();
    let searcher = reader.searcher();
    let query_parser = QueryParser::for_index(&manager.index, vec![
        manager.schema().get_field("metadata_json").unwrap(),
    ]);
    let query = query_parser.parse_query("sunset").unwrap();
    let results = searcher.search(&query, &TopDocs::with_limit(10)).unwrap();
    
    assert_eq!(results.len(), 1);
}
```

## External Docs

Use **ExternalScout** to fetch current Tantivy 0.26 docs for:
- `Schema::builder()` API
- `Index::open_or_create()`
- `IndexWriter` — buffer size, commit pattern
- `IndexReader` — `ReloadPolicy` options
- Query types: `QueryParser`, `TermQuery`, `FuzzyTermQuery`

**Critical:** Tantivy API can vary between minor versions. Always fetch current 0.26 docs.
