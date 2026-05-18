use crate::media_types::{SUPPORTED_EXTENSIONS, is_hidden_path, is_supported_extension};
use std::path::Path;

#[test]
fn test_supported_extensions_include_mov() {
    assert!(
        SUPPORTED_EXTENSIONS.contains(&"mov"),
        "MOV must be in the shared constant to fix the scanner/watcher inconsistency"
    );
}

#[test]
fn test_is_supported_extension_png() {
    assert!(is_supported_extension(Path::new("image.png")));
}

#[test]
fn test_is_supported_extension_jpg() {
    assert!(is_supported_extension(Path::new("photo.jpg")));
    assert!(is_supported_extension(Path::new("photo.jpeg")));
}

#[test]
fn test_is_supported_extension_webp() {
    assert!(is_supported_extension(Path::new("anim.webp")));
}

#[test]
fn test_is_supported_extension_gif() {
    assert!(is_supported_extension(Path::new("anim.gif")));
}

#[test]
fn test_is_supported_extension_video() {
    assert!(is_supported_extension(Path::new("clip.mp4")));
    assert!(is_supported_extension(Path::new("clip.webm")));
    assert!(is_supported_extension(Path::new("clip.mov")));
}

#[test]
fn test_is_supported_extension_case_insensitive() {
    assert!(is_supported_extension(Path::new("photo.PNG")));
    assert!(is_supported_extension(Path::new("photo.JPG")));
    assert!(is_supported_extension(Path::new("clip.MOV")));
    assert!(is_supported_extension(Path::new("clip.Mp4")));
}

#[test]
fn test_is_supported_extension_rejects_txt() {
    assert!(!is_supported_extension(Path::new("readme.txt")));
}

#[test]
fn test_is_supported_extension_rejects_no_extension() {
    assert!(!is_supported_extension(Path::new("Makefile")));
}

#[test]
fn test_is_supported_extension_empty_extension() {
    assert!(!is_supported_extension(Path::new("file.")));
}

// -----------------------------------------------------------------------
// is_hidden_path tests
// -----------------------------------------------------------------------

#[test]
fn test_is_hidden_path_dotfile() {
    assert!(is_hidden_path(Path::new(".hidden.png")));
}

#[test]
fn test_is_hidden_path_dot_directory() {
    assert!(is_hidden_path(Path::new(".hidden/file.png")));
    assert!(is_hidden_path(Path::new("dir/.hidden/file.png")));
}

#[test]
fn test_is_hidden_path_normal_file() {
    assert!(!is_hidden_path(Path::new("normal.png")));
    assert!(!is_hidden_path(Path::new("dir/normal.png")));
}
