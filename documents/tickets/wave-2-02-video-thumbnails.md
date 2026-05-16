# Wave 2.2 — Implement Video Thumbnail Extraction (ffmpeg Keyframe)

| Field | Value |
|-------|-------|
| **Wave** | 2 — Backend: Thumbnail Generation & Media Serving |
| **Seq** | 02 |
| **Estimate** | 2 hours |
| **Depends on** | None (independent utility) |
| **Parallel** | Yes — can run in parallel with 2.1 |

---

## Overview

Extract a thumbnail frame from video files by running `ffmpeg` as a subprocess. Extract a keyframe at a configurable timestamp (default 1 second into the video). The extracted frame is then saved as a PNG and optionally converted to WebP.

## Prerequisites

- `ffmpeg` installed on the system
- Understanding of `tokio::process::Command` for subprocess execution
- Video metadata extraction module (1.5) — optional dependency for duration checking

## Reference Files

- `documents/plans/development-plan.md` — §2 Tech Stack (ffmpeg sidecar, keyframe extraction), §10 Open Questions (Q2: first 1s, configurable, animated thumbnail toggle)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/thumbnails/
├── mod.rs                       # Updated: re-export video module
├── video.rs                     # Video thumbnail extraction
└── video_test.rs                # Co-located tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] `extract_video_thumbnail(source_path, output_dir, timestamp_secs)` extracts a frame from the video
- [ ] Output is a valid PNG image (or WebP, converted from the extracted frame)
- [ ] Default timestamp: 1 second into the video
- [ ] Returns error if ffmpeg is not installed
- [ ] Returns error for invalid/corrupt video files
- [ ] Timeout: ffmpeg killed after 30 seconds
- [ ] Output path follows content-addressed naming convention
- [ ] Works with MP4 and WEBM formats

## Implementation Notes

**ffmpeg command for single frame extraction:**
```bash
ffmpeg -ss 00:00:01 -i input.mp4 -vframes 1 -q:v 2 output.png
```

Where:
- `-ss 00:00:01` — seek to 1 second (before `-i` for fast seeking)
- `-i input.mp4` — input file
- `-vframes 1` — extract exactly 1 frame
- `-q:v 2` — quality setting (2 = high quality)

**Rust implementation:**
```rust
use tokio::process::Command;

pub async fn extract_video_thumbnail(
    source_path: &Path,
    output_dir: &Path,
    timestamp_secs: f64,
) -> Result<PathBuf, Error> {
    let output_path = output_dir.join(format!(
        "{}.png",
        compute_content_hash(source_path).await?
    ));
    
    // Create output directory
    tokio::fs::create_dir_all(output_dir).await?;
    
    let timestamp = format_ffmpeg_timestamp(timestamp_secs);
    
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        Command::new("ffmpeg")
            .args([
                "-ss", &timestamp,
                "-i", source_path.to_str().unwrap(),
                "-vframes", "1",
                "-q:v", "2",
                "-y", // Overwrite output
                output_path.to_str().unwrap(),
            ])
            .output()
    ).await??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::FfmpegError(stderr.to_string()));
    }
    
    Ok(output_path)
}

fn format_ffmpeg_timestamp(seconds: f64) -> String {
    let hours = (seconds / 3600.0) as u32;
    let minutes = ((seconds % 3600.0) / 60.0) as u32;
    let secs = seconds % 60.0;
    format!("{:02}:{:02}:{:06.3}", hours, minutes, secs)
}
```

**Future: animated thumbnails** — Per §10.Q2, animated thumbnails (looping first 5 seconds) are configurable. For v1, implement static frame only. The API should accept a `timestamp_secs` parameter for future flexibility.

**Post-extraction:** If WebP thumbnail is needed (for consistency with image thumbnails), convert the extracted PNG to WebP using the `image` crate:
```rust
let png_path = extract_video_thumbnail(source, output_dir, 1.0).await?;
let img = image::open(&png_path)?;
let webp_path = png_path.with_extension("webp");
img.save(&webp_path)?;
// Optionally delete the intermediate PNG
std::fs::remove_file(&png_path)?;
```

## Test Strategy

```rust
#[tokio::test]
#[ignore = "requires ffmpeg"]
async fn test_extract_mp4_thumbnail() {
    let dir = tempfile::tempdir().unwrap();
    let source = Path::new("../test-fixtures/sample_video.mp4");
    let output_dir = dir.path().join("thumbnails");
    
    let thumb_path = extract_video_thumbnail(&source, &output_dir, 1.0).await.unwrap();
    
    assert!(thumb_path.exists());
    let img = image::open(&thumb_path).unwrap();
    assert!(img.width() > 0);
    assert!(img.height() > 0);
}

#[tokio::test]
async fn test_ffmpeg_not_found_returns_error() {
    // Test with modified PATH (or mock) — returns descriptive error
}

#[tokio::test]
async fn test_invalid_video_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("fake.mp4");
    std::fs::write(&source, b"not a video").unwrap();
    
    let result = extract_video_thumbnail(&source, dir.path(), 1.0).await;
    assert!(result.is_err());
}
```

## External Docs

Use **ExternalScout** to fetch current docs for:
- `ffmpeg` — frame extraction flags (`-ss`, `-vframes`, `-q:v`), fast seeking behavior
