use tantivy::schema::*;

/// Build the Tantivy schema for ImageViz full-text search.
///
/// Matches the schema defined in the development plan §4.
/// Fields are optimised for cursor-based pagination (`id`, `created_at`),
/// text search (`metadata_json`), and display (`width`, `height`).
pub fn build_schema() -> Schema {
    let mut builder = Schema::builder();

    builder.add_text_field("id", STRING | STORED);
    builder.add_text_field("filename", STRING | STORED);
    builder.add_text_field("mime_type", STRING);
    builder.add_text_field("metadata_json", TEXT);
    builder.add_date_field("created_at", INDEXED | FAST);
    builder.add_u64_field("file_size", INDEXED);
    builder.add_u64_field("width", STORED);
    builder.add_u64_field("height", STORED);

    builder.build()
}

#[cfg(test)]
#[path = "schema_test.rs"]
mod tests;
