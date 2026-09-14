# Wave 8.28 — Wave 8 Verification, Docs, and Release

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 28 |
| **Estimate** | 45 minutes |
| **Depends on** | All Wave 8 tickets (8.1–8.27) |
| **Parallel** | No (closing ticket) |
| **Source** | Code review — Verification Checklist; §Recommended Order of Work |

---

## Overview

Closing ticket for Wave 8: run the review's verification checklist end-to-end, confirm every finding is resolved or explicitly deferred, update docs that the wave invalidated (env vars, feature flags, logging behavior), and cut the release version.

## Prerequisites

- 8.1–8.27 all merged

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — Verification Checklist (bottom of document)
- `AGENTS.md` (env vars, debug tooling, architecture notes)
- `README.md`
- `.opencode/context/core/standards/documentation.md`

## Deliverables

```
(verification only + doc updates)
AGENTS.md / README.md       # env vars (CORS_ALLOW_ORIGINS, INDEX_CONCURRENCY, dev-tools feature), logging behavior
CHANGELOG / version bump    # v0.8.0
```

## Acceptance Criteria (Pass/Fail)

**Automated (must be green):**
- [ ] `cd backend && cargo test && cargo clippy -- -D warnings && cargo fmt --check`
- [ ] `cd frontend && npm test && npm run typecheck && npm run lint`
- [ ] `cargo test -- --ignored` (fixtures) and `npx playwright test` (e2e)

**Manual smoke (from the review checklist):**
- [ ] Restart with an existing large library: startup does **not** re-hash unchanged files (8.1)
- [ ] Watch the temp dir: no accumulating `*.webp` files (8.3)
- [ ] RSS after browsing a large grid: lock map + caches bounded (8.6)
- [ ] Settings panel open: no 5s-interval query storm in the network tab (8.19)
- [ ] Search with `sort=score`: arrow-key order matches grid order (8.10)

**Finding audit:**
- [ ] Every review finding (K1–K7, D1–D8, P1–P8, R1–R10) maps to a merged ticket or has a one-line deferral note in the PR description
- [ ] `grep` spot-checks: no `watched_folders` config-key reads (8.4), no `unsafe` in `pool.rs` (8.21), no `formatBytes` (8.23), `StatusCode::INTERNAL_SERVER_ERROR` confined to `error.rs` (8.16)

**Docs:**
- [ ] Env var table updated (added: `INDEX_CONCURRENCY`, `CORS_ALLOW_ORIGINS`; removed/documented: dev-tools feature usage)
- [ ] Architecture notes updated where the wave changed them (thumbnail generation flow, config storage, indexing pipeline)

## Implementation Notes

- Run the smoke checklist against `./scripts/dev.sh` **and** a release build (`cargo build --release` + built frontend) — 8.20 changed what ships in release.
- If any ticket's acceptance criteria could not be verified, file a follow-up ticket instead of silently marking Wave 8 done.

## Test Strategy

This ticket *is* the test strategy — the full suites plus the manual smoke list above. No new code except doc/version changes.
