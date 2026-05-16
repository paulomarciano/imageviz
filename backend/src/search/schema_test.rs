// The test module is nested inside schema::tests, so we need super::super to
// access items from the `search` module (mod.rs) such as IndexManager.
use super::super::*;

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::FieldType;
use tantivy::DateTime;
use tantivy::doc;

// ---------------------------------------------------------------------------
// Schema tests
// ---------------------------------------------------------------------------

#[test]
fn test_schema_has_required_fields() {
    let schema = build_schema();

    // All 8 fields must be present
    let field_names: Vec<&str> = schema.fields().map(|(_, entry)| entry.name()).collect();
    assert!(
        field_names.contains(&"id"),
        "schema should contain 'id'"
    );
    assert!(
        field_names.contains(&"filename"),
        "schema should contain 'filename'"
    );
    assert!(
        field_names.contains(&"mime_type"),
        "schema should contain 'mime_type'"
    );
    assert!(
        field_names.contains(&"metadata_json"),
        "schema should contain 'metadata_json'"
    );
    assert!(
        field_names.contains(&"created_at"),
        "schema should contain 'created_at'"
    );
    assert!(
        field_names.contains(&"file_size"),
        "schema should contain 'file_size'"
    );
    assert!(
        field_names.contains(&"width"),
        "schema should contain 'width'"
    );
    assert!(
        field_names.contains(&"height"),
        "schema should contain 'height'"
    );
    assert_eq!(field_names.len(), 8, "schema should have exactly 8 fields");

    // --- Type + option assertions ---

    // id: STRING | STORED
    let id_entry = schema.get_field_entry(schema.get_field("id").unwrap());
    assert!(matches!(id_entry.field_type(), FieldType::Str(_)));
    assert!(id_entry.is_stored(), "id should be STORED");

    // filename: STRING | STORED
    let fname_entry = schema.get_field_entry(schema.get_field("filename").unwrap());
    assert!(matches!(fname_entry.field_type(), FieldType::Str(_)));
    assert!(fname_entry.is_stored(), "filename should be STORED");

    // mime_type: STRING (not stored, not indexed by default for string)
    let mime_entry = schema.get_field_entry(schema.get_field("mime_type").unwrap());
    assert!(matches!(mime_entry.field_type(), FieldType::Str(_)));

    // metadata_json: TEXT (indexed for full-text)
    let meta_entry = schema.get_field_entry(schema.get_field("metadata_json").unwrap());
    assert!(matches!(meta_entry.field_type(), FieldType::Str(_)));

    // created_at: DATE | INDEXED
    let date_entry = schema.get_field_entry(schema.get_field("created_at").unwrap());
    assert!(matches!(date_entry.field_type(), FieldType::Date(_)));
    assert!(date_entry.is_indexed(), "created_at should be INDEXED");

    // file_size: U64 | INDEXED
    let size_entry = schema.get_field_entry(schema.get_field("file_size").unwrap());
    assert!(matches!(size_entry.field_type(), FieldType::U64(_)));
    assert!(size_entry.is_indexed(), "file_size should be INDEXED");

    // width: U64 | STORED
    let width_entry = schema.get_field_entry(schema.get_field("width").unwrap());
    assert!(matches!(width_entry.field_type(), FieldType::U64(_)));
    assert!(width_entry.is_stored(), "width should be STORED");

    // height: U64 | STORED
    let height_entry = schema.get_field_entry(schema.get_field("height").unwrap());
    assert!(matches!(height_entry.field_type(), FieldType::U64(_)));
    assert!(height_entry.is_stored(), "height should be STORED");
}

// ---------------------------------------------------------------------------
// IndexManager round-trip tests
// ---------------------------------------------------------------------------

#[test]
fn test_index_manager_open_or_create() {
    let dir = tempfile::tempdir().expect("tempdir should succeed");
    let index_path = dir.path().join("tantivy");

    let manager = IndexManager::open_or_create(&index_path)
        .expect("open_or_create should succeed");
    let schema = manager.schema().clone();

    let id = schema.get_field("id").unwrap();
    let filename = schema.get_field("filename").unwrap();
    let mime_type = schema.get_field("mime_type").unwrap();
    let metadata_json = schema.get_field("metadata_json").unwrap();
    let created_at = schema.get_field("created_at").unwrap();
    let file_size = schema.get_field("file_size").unwrap();
    let width = schema.get_field("width").unwrap();
    let height = schema.get_field("height").unwrap();

    let doc = tantivy::doc!(
        id => "test-uuid-1234",
        filename => "hero.png",
        mime_type => "image/png",
        metadata_json => r#"{"prompt":"a hero"}"#,
        created_at => DateTime::from_timestamp_secs(1_700_000_000),
        file_size => 2048u64,
        width => 800u64,
        height => 600u64,
    );

    manager.add_document(doc).expect("add_document should succeed");
    manager.commit().expect("commit should succeed");

    // Search back for the document
    let reader = manager.reader();
    let searcher = reader.searcher();
    let query_parser =
        QueryParser::for_index(manager.index(), vec![filename, metadata_json]);
    let query = query_parser.parse_query("hero").expect("query should parse");
    let collector = TopDocs::with_limit(10).order_by_score();
    let top_docs = searcher
        .search(&query, &collector)
        .expect("search should succeed");

    assert_eq!(top_docs.len(), 1, "should find exactly one document");
}

#[test]
fn test_index_manager_reopen() {
    let dir = tempfile::tempdir().expect("tempdir should succeed");
    let index_path = dir.path().join("tantivy");

    // --- First session: create and add a document ---
    {
        let manager = IndexManager::open_or_create(&index_path)
            .expect("first open_or_create should succeed");
        let schema = manager.schema().clone();

        let doc = tantivy::doc!(
            schema.get_field("id").unwrap() => "persistent-uuid-5678",
            schema.get_field("filename").unwrap() => "persistent.png",
            schema.get_field("mime_type").unwrap() => "image/png",
            schema.get_field("metadata_json").unwrap() => "{}",
            schema.get_field("created_at").unwrap() => DateTime::from_timestamp_secs(1_700_000_000),
            schema.get_field("file_size").unwrap() => 4096u64,
            schema.get_field("width").unwrap() => 1920u64,
            schema.get_field("height").unwrap() => 1080u64,
        );

        manager.add_document(doc).expect("add_document should succeed");
        manager.commit().expect("commit should succeed");
    } // manager drops here → writer thread shuts down cleanly

    // --- Second session: reopen and verify persistence ---
    {
        let manager = IndexManager::open_or_create(&index_path)
            .expect("second open_or_create should succeed");
        let schema = manager.schema().clone();
        let filename = schema.get_field("filename").unwrap();

        let reader = manager.reader();
        let searcher = reader.searcher();
        let query_parser = QueryParser::for_index(manager.index(), vec![filename]);
        let query = query_parser
            .parse_query("persistent.png")
            .expect("query should parse");
        let collector = TopDocs::with_limit(10).order_by_score();
        let top_docs = searcher
            .search(&query, &collector)
            .expect("search should succeed");

        assert_eq!(
            top_docs.len(),
            1,
            "document should persist across IndexManager sessions"
        );
    }
}
