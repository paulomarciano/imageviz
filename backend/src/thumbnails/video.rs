use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

/// Default timestamp (in seconds) for frame extraction when none is specified.
pub const DEFAULT_TIMESTAMP_SECS: u32 = 1;

/// ffmpeg subprocess is killed after this duration.
const FFMPEG_TIMEOUT: Duration = Duration::from_secs(30);

/// Errors that can occur during video thumbnail extraction.
#[derive(Debug)]
pub enum VideoThumbnailError {
    /// Wraps a standard I/O error.
    Io(std::io::Error),
    /// ffmpeg binary was not found on the system PATH.
    FfmpegNotFound,
    /// ffmpeg exited with a non-zero status.
    FfmpegFailed {
        exit_code: Option<i32>,
        stderr: String,
    },
    /// ffmpeg did not complete within the timeout.
    Timeout {
        duration: Duration,
    },
    /// The source video file does not exist.
    SourceNotFound(PathBuf),
    /// ffmpeg ran successfully but did not produce the expected output file.
    InvalidOutput(PathBuf),
}

impl std::fmt::Display for VideoThumbnailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VideoThumbnailError::Io(e) => write!(f, "IO error: {}", e),
            VideoThumbnailError::FfmpegNotFound => {
                write!(f, "ffmpeg is not installed. Please install ffmpeg.")
            }
            VideoThumbnailError::FfmpegFailed { exit_code, stderr } => {
                write!(
                    f,
                    "ffmpeg failed (exit code: {:?}): {}",
                    exit_code,
                    stderr.trim()
                )
            }
            VideoThumbnailError::Timeout { duration } => {
                write!(f, "ffmpeg timed out after {}s", duration.as_secs())
            }
            VideoThumbnailError::SourceNotFound(path) => {
                write!(f, "Source file not found: {}", path.display())
            }
            VideoThumbnailError::InvalidOutput(path) => {
                write!(f, "Output file was not created: {}", path.display())
            }
        }
    }
}

impl std::error::Error for VideoThumbnailError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            VideoThumbnailError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for VideoThumbnailError {
    fn from(e: std::io::Error) -> Self {
        VideoThumbnailError::Io(e)
    }
}

/// Extract a single video frame as a PNG thumbnail using ffmpeg.
///
/// Invokes:
/// `ffmpeg -ss {timestamp} -i {source} -vframes 1 -f image2 {output}`
///
/// The output file is named `{source_stem}_frame_{timestamp}.png` and placed
/// in `output_dir`. The function wraps ffmpeg in a 30-second timeout to
/// prevent hangs on corrupt or problematic files.
///
/// # Errors
///
/// | Variant | Condition |
/// |---|---|
/// | `SourceNotFound` | `source_path` does not exist on disk |
/// | `FfmpegNotFound` | ffmpeg binary is not on `PATH` |
/// | `FfmpegFailed` | ffmpeg exits with non-zero status (stderr attached) |
/// | `Timeout` | ffmpeg does not finish within 30 seconds |
/// | `InvalidOutput` | ffmpeg succeeds but output PNG was not created |
pub async fn extract_video_thumbnail(
    source_path: &Path,
    output_dir: &Path,
    timestamp_secs: u32,
) -> Result<PathBuf, VideoThumbnailError> {
    // -- Precondition: source must exist
    if !source_path.exists() {
        return Err(VideoThumbnailError::SourceNotFound(
            source_path.to_path_buf(),
        ));
    }

    // -- Ensure output directory exists
    tokio::fs::create_dir_all(output_dir).await?;

    // -- Build output path: {stem}_frame_{timestamp}.png
    let stem = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");
    let output_path = output_dir.join(format!("{}_frame_{}.png", stem, timestamp_secs));

    // -- Run ffmpeg with timeout
    let ffmpeg_result = timeout(
        FFMPEG_TIMEOUT,
        Command::new("ffmpeg")
            .args([
                "-ss",
                &timestamp_secs.to_string(),
                "-i",
                &source_path.to_string_lossy(),
                "-vframes",
                "1",
                "-f",
                "image2",
                &output_path.to_string_lossy(),
            ])
            .output(),
    )
    .await;

    match ffmpeg_result {
        Ok(Ok(output)) => {
            if !output.status.success() {
                return Err(VideoThumbnailError::FfmpegFailed {
                    exit_code: output.status.code(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                });
            }
        }
        Ok(Err(e)) => {
            // Command failed to spawn — typically ffmpeg not on PATH
            return if e.kind() == std::io::ErrorKind::NotFound {
                Err(VideoThumbnailError::FfmpegNotFound)
            } else {
                Err(VideoThumbnailError::Io(e))
            };
        }
        Err(_) => {
            return Err(VideoThumbnailError::Timeout {
                duration: FFMPEG_TIMEOUT,
            });
        }
    }

    // -- Verify ffmpeg actually wrote the output file
    if !output_path.exists() {
        return Err(VideoThumbnailError::InvalidOutput(output_path));
    }

    Ok(output_path)
}

#[cfg(test)]
#[path = "video_test.rs"]
mod tests;
