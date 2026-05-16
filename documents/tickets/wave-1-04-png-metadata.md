# Wave 1.4 — Implement PNG Metadata Extraction (tEXt/iTXt Chunks)

| Field | Value |
|-------|-------|
| **Wave** | 1 — Backend: File System Scanner & Metadata Extraction |
| **Seq** | 04 |
| **Estimate** | 2 hours |
| **Depends on** | None (independent utility) |
| **Parallel** | Yes — can run in parallel with 1.5, 1.7 |

---

## Overview

Parse PNG files to extract embedded metadata from `tEXt` and `iTXt` chunks. ComfyUI embeds generation parameters (prompt, workflow) as JSON in these chunks. Extract and return structured Metadata objects.

## Prerequisites

- `png` crate and `serde_json` in Cargo.toml
- Sample ComfyUI PNG files in `test-fixtures/` (for testing)
- `image` crate in Cargo.toml (for dimensions if needed, but primary focus is chunk parsing)

## Reference Files

- `documents/plans/development-plan.md` — §2 Tech Stack (PNG metadata via `png` crate), §7.5 Test Data Strategy (sample PNGs in test-fixtures/)
- `.opencode/context/core/standards/test-coverage.md` — AAA pattern, edge case testing

## Deliverables

```
backend/src/metadata/
├── mod.rs                       # Metadata struct + public exports
├── png.rs                       # PNG chunk parser
└── png_test.rs                  # Co-located tests (co-locate per §7.2)
```

## Acceptance Criteria (Pass/Fail)

- [ ] `parse_png_metadata(path)` extracts text chunks from PNG files
- [ ] Metadata struct includes: `prompt` (the ComfyUI prompt JSON), `workflow` (the ComfyUI workflow JSON)
- [ ] Works with ComfyUI PNGs (real test fixtures from `test-fixtures/`)
- [ ] Returns empty metadata for PNGs with no text chunks (not an error)
- [ ] Returns error for corrupt/invalid PNG files
- [ ] Handles both `tEXt` and `iTXt` chunk types
- [ ] Co-located test: `png_test.rs` tests all above scenarios

## Implementation Notes

**ComfyUI PNG format** — ComfyUI stores metadata in `tEXt` chunks with keyword `"parameters"` (legacy) or `"prompt"` + `"workflow"` (newer). The data is JSON.

**PNG chunk parsing with `png` crate:**
```rust
use png::Decoder;
use std::fs::File;

pub fn parse_png_metadata(path: &Path) -> Result<Metadata, Error> {
    let file = File::open(path)?;
    let decoder = Decoder::new(file);
    let reader = decoder.read_info()?;
    
    let mut metadata = Metadata::default();
    
    for text_chunk in reader.info().uncompressed_latin1_text.iter()
        .chain(reader.info().utf8_text.iter().map(/* convert */))
    {
        match text_chunk.keyword.as_str() {
            "prompt" | "parameters" => {
                metadata.prompt = Some(serde_json::from_str(&text_chunk.text)?);
            }
            "workflow" => {
                metadata.workflow = Some(serde_json::from_str(&text_chunk.text)?);
            }
            _ => {} // Ignore other text chunks
        }
    }
    
    Ok(metadata)
}
```

**Metadata struct:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metadata {
    pub prompt: Option<serde_json::Value>,
    pub workflow: Option<serde_json::Value>,
    // Additional fields for future use
    pub raw_text_entries: HashMap<String, String>,
}
```

**Edge cases to handle:**
- No text chunks at all → `Metadata::default()` (all None)
- Invalid UTF-8 in text chunks → log warning, skip that chunk
- Invalid JSON in prompt/workflow → return the raw string, don't fail
- Very large metadata (some ComfyUI workflows can be 100KB+) → no truncation needed

## Test Strategy

**TDD approach per §7.4:**
1. Write test: `test_parse_comfyui_png_with_metadata()` — uses real fixture file
2. Write test: `test_parse_clean_png_no_metadata()` — PNG with no tEXt chunks
3. Write test: `test_parse_corrupt_png()` — invalid PNG file
4. Write test: `test_parse_png_with_non_json_text()` — tEXt chunk with random text
5. Write test: `test_parse_large_workflow()` — PNG with 100KB+ metadata

**Test fixtures needed** (place in `test-fixtures/`):
- `sample_comfyui.png` — real ComfyUI PNG with prompt + workflow
- `sample_no_metadata.png` — clean PNG with no text chunks

## External Docs

Use **ExternalScout** to fetch current docs for:
- `png` crate — `Decoder`, `Reader`, `OutputInfo`, text chunk access API
