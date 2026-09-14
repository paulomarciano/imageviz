# Wave 8.27 — Reduce Thumbnail Decode Memory Spikes (Single-Pass Downscale)

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 27 |
| **Estimate** | 45 minutes |
| **Depends on** | 8.3 (`thumbnails/image.rs` generation flow rewritten first) |
| **Parallel** | No |
| **Source** | Code review §4 R10 (🔵) |

---

## Overview

`image::open` (`backend/src/thumbnails/image.rs:85-87`) fully decodes the source — a 4K×4K PNG ≈ 64 MB RGBA — before the Lanczos3 `resize`. With 4 concurrent generations (after 8.9, misses only), 200–400 MB transient spikes are normal during a cold grid.

Fix per review: prefer `ImageReader` + `DynamicImage::thumbnail()` — a single-pass downscale that avoids the intermediate full-size buffer — and consider capping decode size for absurdly large sources.

## Prerequisites

- 8.3 merged (write-into-cache flow landed; this reworks the decode step in the same file)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §4 R10
- `backend/src/thumbnails/image.rs:85-87`
- `image` crate docs — `ImageReader`, `DynamicImage::thumbnail` vs `resize`
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/thumbnails/image.rs       # ImageReader::open + thumbnail(); optional max-decode-side guard
backend/src/thumbnails/image_test.rs  # output-equivalence tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Decode path avoids holding the full-size buffer for the resize (uses `thumbnail()`, single pass)
- [ ] Output dimensions, aspect ratio, and WebP encoding unchanged for standard fixtures (golden-file or pixel-tolerance comparison vs current output)
- [ ] Visual quality acceptable on a representative large image (manual spot-check noted in PR)
- [ ] Absurdly large sources (e.g. > 8192px side) are handled: either decodes fine via `thumbnail()`'s streaming sampler or are capped with a documented constant — no OOM path
- [ ] EXIF orientation behavior preserved (if current code applies orientation, keep it)
- [ ] `cargo test` green; `cargo test -- --ignored` green with fixtures

## Implementation Notes

- `DynamicImage::thumbnail` uses a fast box/nearest-class sampler internally tuned for downscaling; `thumbnail` on `ImageBuffer`-backed images avoids materializing the intermediate. If quality regresses noticeably vs Lanczos3, fall back to `resize` but keep the `ImageReader` + limited-decode structure and note the tradeoff in the module doc (the review allows either; memory win comes from not double-buffering).
- Cap constant (if used): `MAX_DECODE_SIDE_PX = 8192`, applied by checking image dimensions from the header (`ImageReader::into_dimensions()`) *before* full decode; larger sources are downscaled via `thumbnail` after decode or rejected with the existing thumbnail-error path.

## Test Strategy

- Fixture (`#[ignore]`): large PNG (generate 6000×4000 via script) → thumbnail at 256/512 → dimensions + format assertions; measure peak allocation only if a profiling harness exists (else code-inspection criterion).
- Equivalence: small/medium fixtures — output within pixel tolerance of the pre-change implementation (keep the old output committed as golden files in the test).
