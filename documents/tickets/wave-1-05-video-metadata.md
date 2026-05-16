# Wave 1.5 — Implement Video Metadata Extraction (ffmpeg Sidecar)

| Field | Value |
|-------|-------|
| **Wave** | 1 — Backend: File System Scanner & Metadata Extraction |
| **Seq** | 05 |
| **Estimate** | 2 hours |
| **Depends on** | None (independent utility) |
| **Parallel** | Yes — can run in parallel with 1.4, 1.7 |

---

## Overview

Extract metadata from video files (MP4, WEBM) by invoking `ffmpeg` as a subprocess. Extract dimensions (width/height), duration, and codec information. This avoids pulling in complex Rust video libraries.

## Prerequisites

- `ffmpeg` installed on the system (available in PATH)
- `tokio::process::Command` for async subprocess execution

## Reference Files

- `documents/plans/development-plan.md` — §2 Tech Stack (ffmpeg sidecar subprocess), §12 video.rs module

## Deliverables

```
backend/src/metadata/
├── mod.rs                       # Updated: re-export video module
├── video.rs                     # ffmpeg metadata extraction
└── video_test.rs                # Co-located tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] `parse_video_metadata(path)` returns `VideoMeta` with `width`, `height`, `duration_ms`
- [ ] Works with MP4 files (test with `test-fixtures/sample_video.mp4`)
- [ ] Works with WEBM files (test with `test-fixtures/sample_video.webm`)
- [ ] Returns error when ffmpeg is not installed (graceful — returns `Err` explaining "ffmpeg not found")
- [ ] Returns error for invalid/corrupt video files
- [ ] Subprocess timeout: ffmpeg killed after 30 seconds (prevents hangs on corrupt files)
- [ ] Does NOT load the entire video into memory — uses `ffprobe` for metadata only

## Implementation Notes

**Use `ffprobe` for metadata** (faster and lighter than `ffmpeg` for info extraction):
```rust
use tokio::process::Command;

pub async fn parse_video_metadata(path: &Path) -> Result<VideoMeta, Error> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
            path.to_str().unwrap(),
        ])
        .output()
        .await?;

    if !output.status.success() {
        return Err(Error::FfmpegError(String::from_utf8_lossy(&output.stderr).to_string()));
    }

    let probe: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    
    // Find the first video stream
    let video_stream = probe["streams"]
        .as_array()
        .and_then(|streams| streams.iter().find(|s| s["codec_type"] == "video"))
        .ok_or(Error::NoVideoStream)?;

    let duration = probe["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse::<f64>().ok())
        .map(|d| (d * 1000.0) as u64);

    Ok(VideoMeta {
        width: video_stream["width"].as_u64().unwrap_or(0) as u32,
        height: video_stream["height"].as_u64().unwrap_or(0) as u32,
        duration_ms: duration,
        codec: video_stream["codec_name"].as_str().map(String::from),
    })
}
```

**VideoMeta struct:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoMeta {
    pub width: u32,
    pub height: u32,
    pub duration_ms: Option<u64>,
    pub codec: Option<String>,
}
```

**Error handling:**
- `ffprobe` not found → return descriptive error ("ffprobe is not installed. Please install ffmpeg.")
- Corrupt file → `ffprobe` returns non-zero exit code → parse stderr for error message
- No video stream → `Error::NoVideoStream` (the file might be audio-only)

**Timeout protection:**
```rust
let output = tokio::time::timeout(
    Duration::from_secs(30),
    Command::new("ffprobe").args([...]).output()
).await??;
```

## Test Strategy

```rust
#[tokio::test]
async fn test_parse_mp4_video() {
    let path = Path::new("../test-fixtures/sample_video.mp4");
    let meta = parse_video_metadata(path).await.unwrap();
    assert!(meta.width > 0);
    assert!(meta.height > 0);
    assert!(meta.duration_ms.is_some());
}

#[tokio::test]
async fn test_parse_webm_video() { ... }

#[tokio::test]
async fn test_ffprobe_not_found() {
    // Temporarily modify PATH, or mock the command
    // (This test can be conditionally skipped in CI if ffmpeg is installed)
}

#[tokio::test]
async fn test_invalid_video_file() {
    let path = Path::new("../test-fixtures/sample_comfyui.png"); // PNG, not video
    let result = parse_video_metadata(path).await;
    assert!(result.is_err());
}
```

**Note:** Tests requiring ffmpeg should be conditional (`#[cfg_attr(not(feature = "ffmpeg"), ignore)]`) so CI can run without ffmpeg installed.

## External Docs

Use **ExternalScout** to fetch:
- `ffprobe` documentation — JSON output format, stream selection, format fields
