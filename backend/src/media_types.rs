/// Supported media file extensions (lowercase, no leading dot).
///
/// Includes both image formats (PNG, JPEG, WebP, GIF) and video formats
/// (MP4, WebM, MOV).  Used by both the scanner and the file watcher so
/// that extensions are defined in a single source of truth.
pub const SUPPORTED_EXTENSIONS: &[&str] =
    &["png", "jpg", "jpeg", "webp", "gif", "mp4", "webm", "mov"];

/// Supported MIME type prefixes for media files.
pub const SUPPORTED_MIME_PREFIXES: &[&str] = &["image/", "video/"];

/// Return `true` if the file at `path` has a supported media extension
/// (case-insensitive).
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use imageviz_backend::media_types::is_supported_extension;
///
/// assert!(is_supported_extension(Path::new("photo.png")));
/// assert!(is_supported_extension(Path::new("clip.mov")));
/// assert!(is_supported_extension(Path::new("clip.MOV")));
/// assert!(!is_supported_extension(Path::new("readme.txt")));
/// assert!(!is_supported_extension(Path::new("Makefile")));
/// ```
pub fn is_supported_extension(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| SUPPORTED_EXTENSIONS.contains(&e.to_lowercase().as_str()))
}

#[cfg(test)]
#[path = "media_types_test.rs"]
mod tests;
