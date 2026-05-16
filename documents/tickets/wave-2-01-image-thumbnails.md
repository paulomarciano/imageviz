# Wave 2.1 — Implement Thumbnail Generator (Image Crate)

| Field | Value |
|-------|-------|
| **Wave** | 2 — Backend: Thumbnail Generation & Media Serving |
| **Seq** | 01 |
| **Estimate** | 2.5 hours |
| **Depends on** | None (independent utility) |
| **Parallel** | Yes — can run in parallel with 2.2 |

---

## Overview

Implement image thumbnail generation using the `image` crate. Given a source image path, produce a WebP thumbnail resized to a configurable target width (default 200px). Use `spawn_blocking` to avoid blocking the async runtime during CPU-bound image processing.

## Prerequisites

- `image` crate in Cargo.toml (with `webp` feature if needed)
- Understanding of `tokio::task::spawn_blocking` for CPU-bound work

## Reference Files

- `documents/plans/development-plan.md` — §2 Tech Stack (image crate, WebP encoding, Lanczos3), §8.1 Performance Targets (<50ms per image), §10 Open Questions (configurable 100-500px)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/thumbnails/
├── mod.rs                       # Public interface
├── image.rs                     # Image thumbnail generation
└── image_test.rs                # Co-located tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] `generate_image_thumbnail(source_path, target_width)` produces a WebP file at the returned path
- [ ] Thumbnail width matches `target_width` (height proportional, maintaining aspect ratio)
- [ ] Output is valid WebP format (can be opened by browser/image viewer)
- [ ] Supports PNG, JPG, WEBP, GIF input formats
- [ ] Uses Lanczos3 filter for quality downscaling
- [ ] Runs in `spawn_blocking` (doesn't block the async runtime)
- [ ] Default target width is 200px (configurable, per §10.Q1 — range 100-500px)
- [ ] Unit test: generate thumbnail, verify width matches target, output is valid WebP

## Implementation Notes

**Thumbnail generation function:**
```rust
use image::{DynamicImage, GenericImageView};
use image::imageops::FilterType;

pub async fn generate_image_thumbnail(
    source_path: &Path,
    output_dir: &Path,
    target_width: u32,
) -> Result<PathBuf, Error> {
    let source = source_path.to_path_buf();
    let output = output_dir.to_path_buf();
    
    tokio::task::spawn_blocking(move || {
        generate_image_thumbnail_sync(&source, &output, target_width)
    }).await?
}

fn generate_image_thumbnail_sync(
    source_path: &Path,
    output_dir: &Path,
    target_width: u32,
) -> Result<PathBuf, Error> {
    let img = image::open(source_path)?;
    
    let (orig_w, orig_h) = img.dimensions();
    let target_height = (target_width as f64 / orig_w as f64 * orig_h as f64) as u32;
    
    let thumbnail = img.resize_exact(target_width, target_height, FilterType::Lanczos3);
    
    // Generate output filename based on content hash
    // (content-addressed → same source always produces same filename)
    let hash = compute_content_hash(source_path)?;
    let output_path = output_dir.join(format!("{}.webp", hash));
    
    // Create output directory if needed
    std::fs::create_dir_all(output_dir)?;
    
    // Save as WebP
    thumbnail.save(&output_path)?;
    
    Ok(output_path)
}
```

**Content-addressed naming** — The thumbnail cache (Wave 2.3) will use content-addressed storage. For now, compute a simple hash of the source file path + modification time to generate unique thumbnail filenames.

**Format support:**
- `image::open()` auto-detects format from file signature (magic bytes)
- PNG → works natively
- JPG → works natively
- WEBP → requires `webp` feature flag on `image` crate
- GIF → take first frame: `img.into_rgba8()` for static thumbnail

**WebP encoding** — The `image` crate supports WebP output with:
```rust
thumbnail.save_with_format(output_path, image::ImageFormat::WebP)?;
```

**Performance** — Lanczos3 is chosen over nearest-neighbor for quality. The trade-off (slightly slower but much better visual quality, especially for images with text/fine details like ComfyUI workflows).

## Test Strategy

```rust
#[tokio::test]
async fn test_generate_png_thumbnail() {
    let dir = tempfile::tempdir().unwrap();
    // Create a 400x300 test PNG using the image crate
    let source = dir.path().join("source.png");
    create_test_image(&source, 400, 300);
    
    let output_dir = dir.path().join("thumbnails");
    let thumb_path = generate_image_thumbnail(&source, &output_dir, 200).await.unwrap();
    
    assert!(thumb_path.exists());
    let thumb = image::open(&thumb_path).unwrap();
    let (w, h) = thumb.dimensions();
    assert_eq!(w, 200);
    assert_eq!(h, 150); // 300 * (200/400) = 150
}
```
