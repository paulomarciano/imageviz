use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::AsyncReadExt;
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
                write!(f, "ffmpeg failed (exit code: {:?}): {}", exit_code, stderr.trim())
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
/// in `output_dir`. The function wraps ffmpeg in a 30-second timeout and
/// kills the subprocess if it exceeds that limit.
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
    // -- Validate source and build output path
    ensure_source_exists(source_path)?;
    tokio::fs::create_dir_all(output_dir).await?;
    let output_path = video_output_path(source_path, output_dir, timestamp_secs);

    // -- Run ffmpeg with timeout
    run_ffmpeg_frame(source_path, &output_path, timestamp_secs).await?;

    // -- Verify ffmpeg actually wrote the output file
    if !output_path.exists() {
        return Err(VideoThumbnailError::InvalidOutput(output_path));
    }

    Ok(output_path)
}

/// Build the deterministic output path for a video thumbnail.
fn video_output_path(source_path: &Path, output_dir: &Path, timestamp_secs: u32) -> PathBuf {
    let stem = source_path.file_stem().and_then(|s| s.to_str()).unwrap_or("video");
    output_dir.join(format!("{}_frame_{}.png", stem, timestamp_secs))
}

/// Ensure the source file exists, or return `SourceNotFound`.
fn ensure_source_exists(path: &Path) -> Result<(), VideoThumbnailError> {
    if path.exists() {
        Ok(())
    } else {
        Err(VideoThumbnailError::SourceNotFound(path.to_path_buf()))
    }
}

/// Spawn ffmpeg, wait with timeout, and kill the child on timeout.
///
/// Uses explicit `spawn()` + `child.wait()` (which takes `&mut self`) so
/// that the child process can be killed when the timeout fires, preventing
/// orphan ffmpeg processes from accumulating. Stderr is captured separately
/// via the pipe before waiting.
async fn run_ffmpeg_frame(
    source_path: &Path,
    output_path: &Path,
    timestamp_secs: u32,
) -> Result<(), VideoThumbnailError> {
    let mut child = Command::new("ffmpeg")
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
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                VideoThumbnailError::FfmpegNotFound
            } else {
                VideoThumbnailError::Io(e)
            }
        })?;

    // Take the stderr pipe before waiting so we can read it after.
    let mut stderr_pipe = child.stderr.take();

    let ffmpeg_result = timeout(FFMPEG_TIMEOUT, child.wait()).await;

    match ffmpeg_result {
        Ok(Ok(status)) => {
            // Read stderr from the pipe (best effort)
            let stderr = read_stderr(&mut stderr_pipe).await;
            if !status.success() {
                return Err(VideoThumbnailError::FfmpegFailed {
                    exit_code: status.code(),
                    stderr,
                });
            }
            Ok(())
        }
        Ok(Err(e)) => Err(VideoThumbnailError::Io(e)),
        Err(_) => {
            // Kill the orphan before returning, then reap the zombie
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err(VideoThumbnailError::Timeout { duration: FFMPEG_TIMEOUT })
        }
    }
}

/// Read the remaining bytes from an optional stderr pipe into a string.
async fn read_stderr(pipe: &mut Option<tokio::process::ChildStderr>) -> String {
    match pipe {
        Some(p) => {
            let mut buf = String::new();
            let _ = p.read_to_string(&mut buf).await;
            buf
        }
        None => String::new(),
    }
}

#[cfg(test)]
#[path = "video_test.rs"]
mod tests;
