# Wave 1.6 — Implement File Type Detection (MIME + Dimensions)

| Field | Value |
|-------|-------|
| **Wave** | 1 — Backend: File System Scanner & Metadata Extraction |
| **Seq** | 06 |
| **Estimate** | 1.5 hours |
| **Depends on** | 1.4 (PNG metadata), 1.5 (video metadata) |
| **Parallel** | No |

---

## Overview

Build a unified detection module that, given any file path, determines its MIME type, dimensions (width/height), and file size. This combines the image dimension reading (from `image` crate) with the video dimension reading (from 1.5) into a single dispatch function.

## Prerequisites

- PNG metadata module (1.4) — for PNG dimension extraction
- Video metadata module (1.5) — for video dimension extraction
- `image` crate in Cargo.toml
- `mime_guess` crate in Cargo.toml

## Reference Files

- `documents/plans/development-plan.md` — §2 Tech Stack (image crate, mime_guess), §12 detect.rs module
- `.opencode/context/development/principles/clean-code.md`

## Deliverables

```
backend/src/metadata/
├── mod.rs                       # Updated: re-export detect
├── detect.rs                    # MIME + dimension detection
└── detect_test.rs               # Co-located tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] `detect_media(path)` returns `MediaInfo` with `mime_type`, `width`, `height`, `file_size`
- [ ] Correct MIME types: PNG → `image/png`, JPG → `image/jpeg`, WEBP → `image/webp`, GIF → `image/gif`, MP4 → `video/mp4`, WEBM → `video/webm`
- [ ] Dimensions extracted correctly for PNG, JPG, WEBP, GIF
- [ ] Dimensions extracted correctly for MP4, WEBM (via ffprobe — async)
- [ ] Returns error for unsupported file types
- [ ] Returns `width: None, height: None` if dimensions can't be determined (not an error)
- [ ] File size always populated (from `std::fs::metadata`)

## Implementation Notes

**MediaInfo struct:**
```rust
#[derive(Debug, Clone, Serialize)]
pub struct MediaInfo {
    pub mime_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub file_size: u64,
}
```

**Detection dispatch:**
```rust
pub async fn detect_media(path: &Path) -> Result<MediaInfo, Error> {
    let extension = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let mime_type = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => return Err(Error::UnsupportedFormat(extension)),
    }.to_string();

    let file_size = std::fs::metadata(path)?.len();

    let (width, height) = if mime_type.starts_with("image/") {
        detect_image_dimensions(path)?
    } else {
        let video_meta = parse_video_metadata(path).await?;
        (Some(video_meta.width), Some(video_meta.height))
    };

    Ok(MediaInfo { mime_type, width, height, file_size })
}
```

**Image dimension detection** — use the `image` crate for fast dimension reading (without decoding the full image):
```rust
fn detect_image_dimensions(path: &Path) -> Result<(Option<u32>, Option<u32>), Error> {
    let reader = image::io::Reader::open(path)?
        .with_guessed_format()?;
    let (w, h) = reader.into_dimensions()?;
    Ok((Some(w), Some(h)))
}
```

**Note:** The `image` crate's `into_dimensions()` reads only the header, not the full image — it's fast.

**MIME type decision** — Based on file extension primarily (not magic bytes), since the scanner already filters by extension. `mime_guess` crate can be used as a fallback but extension-based dispatch is simpler and sufficient for this use case.

## Test Strategy

```rust
#[tokio::test]
async fn test_detect_png() {
    let path = Path::new("../test-fixtures/sample_comfyui.png");
    let info = detect_media(path).await.unwrap();
    assert_eq!(info.mime_type, "image/png");
    assert!(info.width.unwrap() > 0);
    assert!(info.height.unwrap() > 0);
    assert!(info.file_size > 0);
}

#[tokio::test]
async fn test_detect_unsupported() {
    let path = Path::new("test.txt");
    let result = detect_media(path).await;
    assert!(result.is_err());
}
```
