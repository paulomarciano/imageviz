use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoMeta {
    pub width: u32,
    pub height: u32,
    pub duration_ms: Option<u64>,
    pub codec: Option<String>,
}

#[derive(Debug)]
pub enum VideoError {
    FfmpegNotFound,
    FfmpegError(String),
    NoVideoStream,
    Timeout,
    Io(std::io::Error),
    Json(serde_json::Error),
    InvalidPath,
}

impl std::fmt::Display for VideoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VideoError::FfmpegNotFound => {
                write!(f, "ffprobe is not installed. Please install ffmpeg.")
            }
            VideoError::FfmpegError(e) => write!(f, "ffprobe error: {}", e),
            VideoError::NoVideoStream => write!(f, "No video stream found in file"),
            VideoError::Timeout => write!(f, "ffprobe timed out after 30 seconds"),
            VideoError::Io(e) => write!(f, "IO error: {}", e),
            VideoError::Json(e) => write!(f, "JSON parse error: {}", e),
            VideoError::InvalidPath => write!(f, "Invalid file path"),
        }
    }
}

impl std::error::Error for VideoError {}

impl From<std::io::Error> for VideoError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            VideoError::FfmpegNotFound
        } else {
            VideoError::Io(e)
        }
    }
}

impl From<serde_json::Error> for VideoError {
    fn from(e: serde_json::Error) -> Self {
        VideoError::Json(e)
    }
}

/// Parse video metadata using ffprobe.
///
/// Invokes `ffprobe` as a subprocess with `-v quiet -print_format json -show_format
/// -show_streams` to extract metadata without loading the entire video into memory.
///
/// # Errors
///
/// Returns `VideoError::FfmpegNotFound` if ffprobe is not on PATH.
/// Returns `VideoError::Timeout` if ffprobe doesn't complete within 30 seconds.
/// Returns `VideoError::NoVideoStream` if the file has no video stream (e.g. audio-only).
/// Returns `VideoError::FfmpegError` if ffprobe exits with a non-zero status.
pub async fn parse_video_metadata(path: &Path) -> Result<VideoMeta, VideoError> {
    let path_str = path.to_str().ok_or(VideoError::InvalidPath)?;

    let output = tokio::time::timeout(
        Duration::from_secs(30),
        Command::new("ffprobe")
            .args([
                "-v",
                "quiet",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
                path_str,
            ])
            .output(),
    )
    .await
    .map_err(|_| VideoError::Timeout)??;

    if !output.status.success() {
        // When ffprobe is not installed, the OS returns "command not found"
        // on stderr. Treat any non-zero exit as an ffprobe error.
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(VideoError::FfmpegError(stderr.to_string()));
    }

    let probe: serde_json::Value = serde_json::from_slice(&output.stdout)?;

    // Find the first video stream in the streams array
    let video_stream = probe["streams"]
        .as_array()
        .and_then(|streams| streams.iter().find(|s| s["codec_type"] == "video"))
        .ok_or(VideoError::NoVideoStream)?;

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

#[cfg(test)]
#[path = "video_test.rs"]
mod tests;
