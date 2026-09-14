# Wave 8.5 — Support .mov Detection (ffprobe Video Path)

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 05 |
| **Estimate** | 1 hour |
| **Depends on** | — |
| **Parallel** | Yes |
| **Source** | Code review §3 P2 (🟡) |

---

## Overview

`"mov"` is in `SUPPORTED_EXTENSIONS` (`backend/src/media_types.rs:6-7`), so the scanner accepts `.mov` files — but `detect_media` keeps its own hardcoded list with no `"mov"` arm (`backend/src/metadata/detect.rs:22-30`), returning `UnsupportedFormat`. Result: every `.mov` file found at scan time costs a failed detection + an error log + a `stats.errors` increment, **every startup**, and the file never appears in the gallery.

Preferred fix: add an ffprobe-based video path for `mov` in `detect_media` (QuickTime from modern cameras/phones is common). Longer term, derive the detection arms from the shared extension list so this class of drift cannot recur.

## Prerequisites

- None (v0.7.0 baseline)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §3 P2
- `backend/src/media_types.rs` — `SUPPORTED_EXTENSIONS` shared constant
- `backend/src/metadata/detect.rs:22-30` — hardcoded detection list
- `backend/src/metadata/video.rs` — existing ffprobe extraction
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/metadata/detect.rs        # mov arm routed through ffprobe video detection
backend/src/metadata/detect_test.rs   # table-driven extension coverage test
```

## Acceptance Criteria (Pass/Fail)

- [ ] A `.mov` file indexes successfully with video MIME type, dimensions, and duration (fixture test)
- [ ] Scanner no longer logs detection errors or increments `stats.errors` for `.mov` files
- [ ] `.mov` files appear in the gallery grid and are playable in the detail view
- [ ] Table-driven test: every extension in `SUPPORTED_EXTENSIONS` has a defined detection outcome (no silent `UnsupportedFormat` for a listed extension)
- [ ] `cargo test` green; `cargo test -- --ignored` green after `./scripts/generate-fixtures.sh`

## Implementation Notes

- Route the `mov` arm through the same ffprobe path used for mp4/webm — QuickTime metadata (moov atom placement) sometimes needs `ffmpeg` remux handling; verify the existing video thumbnail pipeline (2.2) also produces a poster frame for `.mov`.
- For the drift guard, add a compile-time-coupled test rather than refactoring the enum now:

```rust
#[test]
fn every_supported_extension_has_a_detection_path() {
    for ext in SUPPORTED_EXTENSIONS {
        assert!(
            DETECTABLE_EXTENSIONS.contains(ext) || KNOWN_UNSUPPORTED.contains(ext),
            "{ext} is scannable but has no detection arm"
        );
    }
}
```

- If a listed extension genuinely cannot be detected (none today after this ticket), it must be explicit in `KNOWN_UNSUPPORTED`, not implicit.

## Test Strategy

- Unit: extension → expected `MediaType` mapping (no fs access).
- Fixture (`#[ignore]`): generate a small `.mov` via `generate-fixtures.sh`, run `detect_media` + `process_file_metadata`, assert video metadata extracted; run the full indexer over a folder containing it and assert zero `stats.errors`.
