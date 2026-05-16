use png::Decoder;
use png::text_metadata::{ITXtChunk, TEXtChunk};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metadata {
    pub prompt: Option<serde_json::Value>,
    pub workflow: Option<serde_json::Value>,
    #[serde(default)]
    pub raw_text_entries: HashMap<String, String>,
}

#[derive(Debug)]
pub enum PngParseError {
    Io(std::io::Error),
    Png(png::DecodingError),
}

impl std::fmt::Display for PngParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PngParseError::Io(e) => write!(f, "IO error: {}", e),
            PngParseError::Png(e) => write!(f, "PNG decoding error: {}", e),
        }
    }
}

impl std::error::Error for PngParseError {}

impl From<std::io::Error> for PngParseError {
    fn from(e: std::io::Error) -> Self {
        PngParseError::Io(e)
    }
}

impl From<png::DecodingError> for PngParseError {
    fn from(e: png::DecodingError) -> Self {
        PngParseError::Png(e)
    }
}

/// Parse PNG file and extract metadata from tEXt/iTXt chunks.
///
/// Reads all text chunks (both pre-IDAT and post-IDAT via `reader.finish()`)
/// and extracts ComfyUI-style metadata: `prompt`, `parameters`, and `workflow`
/// keyword values as JSON, with all raw entries stored in `raw_text_entries`.
pub fn parse_png_metadata(path: &Path) -> Result<Metadata, PngParseError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let decoder = Decoder::new(reader);
    let mut reader = decoder.read_info()?;

    let mut metadata = Metadata::default();

    // Process uncompressed Latin-1 text chunks (tEXt) — pre-IDAT
    extract_text_chunks(&reader.info().uncompressed_latin1_text, &mut metadata);
    // Process UTF-8 text chunks (iTXt) — pre-IDAT
    extract_itext_chunks(&reader.info().utf8_text, &mut metadata);

    // Consume trailing data to capture text chunks placed after IDAT
    // Some PNG encoders (including some ComfyUI configurations) write
    // metadata chunks after the image data.
    let _ = reader.finish();

    // Process tEXt chunks found after IDAT
    extract_text_chunks(&reader.info().uncompressed_latin1_text, &mut metadata);
    // Process iTXt chunks found after IDAT
    extract_itext_chunks(&reader.info().utf8_text, &mut metadata);

    Ok(metadata)
}

/// Extract metadata from uncompressed Latin-1 (tEXt) text chunks.
fn extract_text_chunks(chunks: &[TEXtChunk], metadata: &mut Metadata) {
    for text_chunk in chunks {
        let key = text_chunk.keyword.clone();
        let value = text_chunk.text.clone();

        // Avoid duplicate entries from pre/post-IDAT chunks
        if metadata.raw_text_entries.contains_key(&key) {
            continue;
        }

        metadata.raw_text_entries.insert(key.clone(), value.clone());
        handle_metadata_key(&key, &value, metadata);
    }
}

/// Extract metadata from UTF-8 (iTXt) text chunks.
///
/// iTXt chunks have a private `text` field; access via `get_text()`.
/// Handles both compressed and uncompressed iTXt chunks.
fn extract_itext_chunks(chunks: &[ITXtChunk], metadata: &mut Metadata) {
    for text_chunk in chunks {
        let key = text_chunk.keyword.clone();

        // Avoid duplicate entries
        if metadata.raw_text_entries.contains_key(&key) {
            continue;
        }

        // `text` is private in ITXtChunk — must use get_text()
        if let Ok(value) = text_chunk.get_text() {
            metadata.raw_text_entries.insert(key.clone(), value.clone());
            handle_metadata_key(&key, &value, metadata);
        }
    }
}

/// Attempt to parse known ComfyUI metadata keys as JSON.
///
/// Known keys: "prompt", "parameters" (legacy), "workflow".
/// Falls back to storing the raw string as a JSON string value if
/// the content is not valid JSON.
fn handle_metadata_key(key: &str, value: &str, metadata: &mut Metadata) {
    match key {
        "prompt" | "parameters" => {
            if metadata.prompt.is_none() {
                metadata.prompt = match serde_json::from_str(value) {
                    Ok(json) => Some(json),
                    Err(_) => {
                        // Some ComfyUI prompts are plain text, not JSON
                        Some(serde_json::Value::String(value.to_owned()))
                    }
                };
            }
        }
        "workflow" => {
            if metadata.workflow.is_none() {
                // Only store if valid JSON, silently skip otherwise
                if let Ok(json) = serde_json::from_str(value) {
                    metadata.workflow = Some(json);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "png_test.rs"]
mod tests;
