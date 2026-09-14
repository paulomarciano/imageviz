# ImageViz — Code Review: KISS · DRY · Performance · Resource Load

> **Version**: 1.0
> **Date**: 2026-09-05
> **Scope**: Full-stack audit (backend `backend/src/`, frontend `frontend/src/`)
> **Baseline**: v0.7.0, all 8 development waves complete
> **Relation to prior plan**: This is a fresh audit. Most items in
> `documents/plans/performance-maintainability-improvement-plan.md` (v2.0) are now
> verified as implemented (route split, watcher pipeline, shared extensions constant,
> DashMap locks, LTO release profile, background eviction timer, `fs2` disk check,
> focus-trap hook, cursor-pagination hook, ref-based image drag, SSE pruning,
> conditional query mounting, docs). Findings below are **new** or **still open**.

**Assessment**: The codebase is in good shape — clean module boundaries, consistent
error handling, thorough tests, and several earlier review items already landed.
The findings that remain concentrate in four areas: two cold-start/data-model
problems that dominate performance at the 100K–1M file scale, two unbounded
memory/disk growth paths in the thumbnail cache, a watched-folder dual source of
truth that generates duplication across five modules, and always-on dev tooling
(tokio-console, pprof) that contradicts the "gallery app should be almost
invisible" goal.

---

## Summary of Top Findings

| # | Severity | Dimension | Finding |
|---|----------|-----------|---------|
| 1 | 🔴 | Performance | Startup re-hashes **every file, sequentially**, on every launch; the incremental (size+mtime skip) indexer exists but is never called |
| 2 | 🔴 | Resource | Thumbnail lock map (`LOCKS` DashMap) grows unboundedly with library size and is never cleaned |
| 3 | 🔴 | Resource | Second thumbnail cache in `/tmp` is never cleaned — disk usage silently doubles |
| 4 | 🔴 | DRY | Watched folders stored twice (JSON blob + table), causing duplicated resolution logic and legacy fallbacks in 5 modules |
| 5 | 🟡 | Performance | `.mov` accepted by scanner but rejected by `detect_media` — never indexable, errors every startup |
| 6 | 🟡 | Resource | tokio-console layer + pprof endpoint always compiled in and always active in release builds |
| 7 | 🟡 | Performance | Tantivy search traverses the index twice per query (Count + TopDocs); `MultiCollector` does it in one |
| 8 | 🟡 | Performance/DRY | `App.tsx` runs duplicate data hooks with *different parameters* than the grid (extra requests + wrong nav order) |

---

## 1. KISS — Simple is better than complex

### 🔴 K1 · Watched folders have two sources of truth

**Files**: `backend/src/config/mod.rs`, `backend/src/routes/media/file.rs:68-91`,
`backend/src/watcher/handler.rs:275-284`, `backend/src/db/schema.rs`

Folder configuration is persisted **twice**: as a JSON blob in `config(key='watched_folders')`
*and* as rows in the `watched_folders` table. Every consumer must decide which to read:

- `resolve_media_path()` first tries the table, then **falls back to parsing the JSON blob** (file.rs:68-91) — 30 extra lines of legacy path on the hottest media-serving route.
- `load_watched_folders()` (handler.rs:275) reads the JSON, not the table.
- `assign_folder_ids()` (config/mod.rs:48-79) contains a comment describing a production log-flood bug caused *precisely* by the two stores drifting out of sync.
- `update_config()` writes both, in a specific order, to keep them aligned.

**Fix**: Make the `watched_folders` table the single source of truth. Derive
`AppConfig` from the table (`SELECT id, path, label FROM watched_folders`) and
delete the JSON blob plus the fallback in `resolve_media_path`. This alone
removes ~60 lines and an entire class of sync bugs.

### 🟡 K2 · Unreachable failure mode in `IndexManager`

**File**: `backend/src/search/mod.rs:29,102,115,149,170`

The writer is stored as `Mutex<Option<IndexWriter>>`, and every method maps
`None` to the error `"IndexWriter has been consumed"`. Nothing ever calls
`.take()` — the `Option` is never consumed, so the error is unreachable and
every accessor pays an `ok_or` check for it.

**Fix**: Store `Mutex<IndexWriter>` directly. Deletes ~12 lines and one
impossible state.

### 🟡 K3 · Dev tooling is unconditionally wired into the production binary

**Files**: `backend/src/main.rs:33-46,209`, `backend/Cargo.toml`

- `ConsoleLayer::new()` is constructed and the console server task spawned on
  **every** start, regardless of `TOKIO_CONSOLE_ADDR`. The layer instruments
  every task spawned afterward (this is why the build needs `--cfg tokio_unstable`).
- `/debug/pprof` is mounted on every start. `profiler.rs`'s own doc comment says
  it "should not be exposed in production" and suggests "a compile-time feature
  flag" — which hasn't been done.

**Fix**: Gate both behind a feature (`dev-tools`) or `#[cfg(debug_assertions)]`.
Release binaries get smaller, `tokio_unstable` becomes unnecessary for releases,
and the runtime loses per-task instrumentation overhead.

### 🟡 K4 · Redundant `unsafe impl Send/Sync`

**File**: `backend/src/db/pool.rs:77-78`

`SqliteConnectionManager` contains only `Option<PathBuf>` and `bool` — the
compiler already derives `Send + Sync` for it. The manual `unsafe impl` blocks
add unsafe code that does nothing and mislead readers into thinking a
non-`Sync` `Connection` is stored in the manager.

**Fix**: Delete both `unsafe impl` lines and the safety comment.

### 🟡 K5 · In-memory test pool is a latent correctness trap

**File**: `backend/src/db/pool.rs:97-101`

Each connection in the in-memory r2d2 pool (max 3) opens a **separate, empty**
SQLite database. Two concurrent `pool.get()` calls see different data, and a
connection created after migrations ran has no tables. Tests only pass because
usage happens to be sequential. The day a test holds two connections, it will
fail mysteriously.

**Fix**: Use a single-connection pool (`max_size(1)`) or `sqlite:?cache=shared`
URI semantics for the in-memory variant.

### 🔵 K6 · Dead and redundant code

| Item | Location | Note |
|------|----------|------|
| `IndexManager::refresh()` | `search/mod.rs:124-126` | Alias for `commit()`, zero callers |
| `compute_file_hash_blocking()` | `scanner/hasher.rs:43-45` | Only its own test calls it |
| `extract_png_metadata()` wrapper | `metadata/detect.rs:67-69` | Pure pass-through, only tests call it |
| `search::indexer::incremental_index()` | `search/indexer.rs:154-261` | Only tests call it (see P1 for the design question this raises) |
| `Json(json!(row))` double serialization | `routes/media/detail.rs:82` | Serializes the struct to a `Value`, then serializes the `Value` again — return `Json(row)` (wrapped with meta) directly |
| `es.onmessage` handler | `frontend/src/hooks/use-sse.ts:61-76` | Backend only sends *named* SSE events, which never trigger `onmessage`; the branch is effectively dead |

### 🔵 K7 · Minor simplifications

- `routes/media/list.rs:99-135`: the dynamic SQL builder (`where_parts` +
  manual `?N` counting + `Vec<&dyn ToSql>` mapping) can be replaced by
  `rusqlite::params_from_iter` over a plain `Vec<Value>`, removing the
  counter bookkeeping and the `param_refs` mapping. Also `let mut items = { … let mut items … }` — the outer `mut` is redundant (line 137).
- `routes/config.rs:102-117`: `save_config()` is called explicitly and then
  again inside `assign_folder_ids()` — one redundant DB write per PUT.
- `frontend/src/hooks/use-sse.ts` + `client.ts`: `saveConfig` in
  `config-panel.tsx:24-34` uses raw `fetch` while everything else goes through
  the typed client — add a `put<T>()` helper next to `get<T>()`.
- `Cargo.toml`: `tokio = { features = ["full"] }` pulls in process, signal,
  io-util, etc. Enumerate only what's used (`rt-multi-thread`, `macros`,
  `signal`, `fs`, `sync`, `time`, `net`) — smaller binary, faster builds.

---

## 2. DRY — Don't repeat yourself

### 🔴 D1 · Watched-folder dual storage (see K1)

The duplication is not just data: its **resolution logic** is duplicated in
`resolve_media_path` (file.rs), `load_watched_folders` (handler.rs), and
`folder_id_map` (config/mod.rs) — three functions that all answer "which folder
does this path/row belong to". Consolidating storage collapses all three.

### 🟡 D2 · `full_index` vs `incremental_index` — ~150 duplicated lines

**File**: `backend/src/indexer/mod.rs:47-124 vs 136-237`

Both functions repeat: assign-folder-ids block, `scan_all_folders`, the
empty-library early return, the `chunks(BATCH_SIZE)` loop with identical
`store_file`/stats/error handling, and the `remove_deleted_items` epilogue.
The only difference is the skip check inside the loop.

**Fix**: Extract one `run_index(pool, config, progress, skip: impl Fn(&FolderFileEntry) -> bool)`
core; `full_index` passes `|_| false`. Cuts the file roughly in half and
guarantees the two paths can't drift.

### 🟡 D3 · `full_reindex` vs `incremental_index` (Tantivy side) — 80% duplicated

**File**: `backend/src/search/indexer.rs:60-142 vs 154-261`

The 8 field lookups, row-mapping closure, `tantivy::doc!` construction, and
error accounting are verbatim copies. **Fix**: extract `index_rows(rows, manager)`
and let both entry points feed it. (If D4/P1 lead to deleting the Tantivy-side
`incremental_index`, this resolves itself.)

### 🟡 D4 · PNG metadata extraction logic duplicated

**Files**: `backend/src/indexer/mod.rs:290-302` and `backend/src/watcher/stages/extract.rs:50-61`

Identical condition (`prompt.is_some() || workflow.is_some() || !raw_text_entries.is_empty()`)
plus identical serialization, with a comment in `extract.rs` saying it "must
match the indexer's logic" — a comment that exists because the code is
duplicated. **Fix**: one `pub fn metadata_to_json(meta: &Metadata) -> Option<String>`
in `metadata/png.rs`, called from both.

### 🟡 D5 · Timestamp formatting duplicated

**Files**: `backend/src/scanner/walker.rs:95-101` (`datetime_to_iso`) and
`backend/src/watcher/handler.rs:264-269` (`system_time_to_iso`) — byte-for-byte
equivalent. **Fix**: keep one (e.g. in `media_types.rs` or a small `util` module).

### 🟡 D6 · Response-type and error-tuple duplication across routes

- `MediaItemSummary` is defined twice with identical fields:
  `routes/media/list.rs:31-43` and `routes/search.rs:264-276`. Move to a shared
  `routes/response.rs`.
- The `(StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": …})))` tuple is
  hand-written **~20 times** (grep confirms), and the "Failed to acquire
  database connection" pool-error block opens nearly every handler. Introduce a
  small `AppError` enum (or helper fns `internal_error()` / `pool_error()`) with
  `IntoResponse`, and handlers become `?`-propagating instead of `map_err`-noise.

### 🟡 D7 · Frontend: duplicated data-fetching layer in `App.tsx`

**Files**: `frontend/src/App.tsx:26-67` vs `frontend/src/components/media/thumbnail-grid.tsx:42-125`

`ActiveViewContent` calls `useInfiniteMedia`/`useSearch` so the detail view can
navigate, while `ThumbnailGrid` internally calls **the same hooks again**. This
is duplicated data-layer code *with drift*: App.tsx hardcodes
`useSearch(query, 100, undefined, 'recency')` — ignoring the mime filter and
sort atoms the grid uses.

Consequences (beyond DRY — see P8): in search mode with a non-default sort or
mime filter, **two different search queries are executed per keystroke**, and
arrow-key navigation in the detail view follows a *different order than the
grid displays*.

**Fix**: lift the fetched data into a Jotai atom (or a small context) written by
the grid, and have `App.tsx` read it for `DetailView`. Delete the duplicate
hooks from `ActiveViewContent`.

### 🔵 D8 · Small frontend duplications

- `formatBytes()` in `config-panel.tsx:9-14` duplicates `formatFileSize()` from
  `utils/format.ts` — and has already drifted: the shared one lacks the GB case
  that the local one has. Extend `formatFileSize` and delete the local copy.
- Escape-to-close key handlers are independently implemented in
  `config-panel.tsx:52-58`, `shortcuts-panel.tsx:63-70`, and
  `detail-view.tsx:57-74`. Fold into `useFocusTrap` (add an `onClose` option)
  or a two-line `useEscape(onClose)` hook.
- The indexing pipeline (Phase 1 SQLite + Phase 2 Tantivy reindex on a read-only
  connection) exists twice: `main.rs:295-393` and `routes/config.rs:160-200`.
  Extract `run_indexing_pipeline(pool, config, progress, im, db_path)`.

---

## 3. Performance — as fast as possible

### 🔴 P1 · Every app start re-hashes the entire library — sequentially

**Files**: `backend/src/main.rs:320` (calls `full_index`),
`backend/src/indexer/mod.rs:47-124,136-237`

`full_index` computes a SHA-256 of **every file on every startup** — the skip
check (`store_file`, comparing checksums) happens only *after* hashing. An
`incremental_index` that skips files whose size+mtime are unchanged **already
exists** (indexer/mod.rs:136) but is called by nothing except its own test.
For the target dataset (100K–1M files), that is potentially hours of redundant
disk I/O and CPU per restart, on a machine that is supposed to be running a
lightweight gallery.

Compounding it, Phase 1 processes files **one at a time**
(`for ff_entry in chunk { process_file_metadata(...).await }` — indexer/mod.rs:83-92):
hash + ffprobe for 1 file, then the next. On any multicore machine this
under-utilizes the CPU by ~Nx.

**Fix** (two parts, independent):
1. Make startup call the incremental path (mtime+size gate before hashing).
2. Parallelize Phase 1 with `futures::stream::iter(pairs).map(process_file_metadata).buffer_unordered(N)`
   (N = a few cores; reuse the `ThumbnailLimiter` pattern if a cap is wanted).

These two changes together are the single largest win in this report — likely
**10–50× faster warm starts** on large libraries.

### 🟡 P2 · `.mov` files pass the scanner, fail detection, never get indexed

**Files**: `backend/src/media_types.rs:6-7` (includes `"mov"`) vs
`backend/src/metadata/detect.rs:22-30` (no `"mov"` arm → `UnsupportedFormat` error)

The shared-extensions fix (old plan item 4.3) unified the scanner and watcher,
but `detect_media` kept its own hardcoded list — so every `.mov` file found at
scan time costs a failed detection + an error log + a `stats.errors` increment,
every startup. The file never appears in the gallery.

**Fix**: either add an ffprobe-based video path for `mov` in `detect_media`
(preferred — QuickTime from modern cameras/phones is common), or drop `"mov"`
from `SUPPORTED_EXTENSIONS` so it isn't scanned at all. Longer term, derive the
extension list from one enum used by both modules.

### 🟡 P3 · Search executes two full index traversals per query

**File**: `backend/src/routes/search.rs:152-200`

`searcher.search(&query, &Count)` walks all matching docs to compute the total,
then `searcher.search(&query, &TopDocs…)` walks them again. Tantivy's
`MultiCollector` returns count + top-docs in a **single** traversal.

**Fix**:
```rust
let multicollector = MultiCollector::new()
    .with_collector(top_docs_collector)
    .with_collector(Count);
let (count_guard, mut docs_guard) = searcher.search(&query, &multicollector)?;
```
Halves the per-keystroke search cost on large indexes.

### 🟡 P4 · Total-count caching is asymmetric and briefly serializes requests

**File**: `backend/src/routes/media/list.rs:76-96`

- The 30s cache covers only the unfiltered count. A mime-filtered page load
  runs `COUNT(*) … WHERE mime_type LIKE ?` on **every request** — a full index
  scan per page on 1M rows (and SQLite won't use `idx_media_mime` for a
  case-insensitive `LIKE` prefix by default).
- The unfiltered COUNT query is executed **while holding** the
  `std::sync::Mutex` guard (list.rs:84-95): concurrent list requests block on a
  std mutex held across a potentially long query, on the async runtime.

**Fix**: key the cache by the mime filter (a 1-entry cache per filter or a tiny
map with the same 30s TTL), and compute the count **before** locking; hold the
mutex only to store the result. Consider `std::sync::Mutex` → `parking_lot`-free
pattern or just keep it short (it's fine once the query is outside).

### 🟡 P5 · Thumbnail semaphore gates cache hits too

**File**: `backend/src/routes/media/thumbnail.rs:46-51`

`thumbnail_limiter.acquire()` (4 permits) is taken **before** the cache lookup.
A grid of cached thumbnails is served at most 4-at-a-time, and cache hits queue
behind in-flight CPU-bound video generations. The cache's own per-key locking
(cache.rs) already prevents duplicate generation.

**Fix**: make `get_or_generate_thumbnail` return a "was cached" signal, or
probe the cache path in the route first and only acquire the permit on a miss.
Cached responses then bypass the limiter entirely.

### 🟡 P6 · Media path resolution: 3 queries + blocking `exists()` per request

**Files**: `backend/src/routes/media/file.rs:22-92`, `routes/media/thumbnail.rs:34-43`

- `resolve_media_path` issues: (1) row lookup, (2) `folder_id` lookup of the
  *same row*, (3) watched-folder lookup, then possibly (4) config-JSON parsing
  (legacy). One `LEFT JOIN` on `watched_folders` returns everything.
- `serve_file` and `serve_thumbnail` each run a **second** query against the
  same row for `checksum`/`modified_at`. Fold into the same statement.
- `full.exists()` and the fallback-loop `exists()` are **blocking** `std::fs`
  calls inside async handlers on the request path (file.rs:62,86) — use
  `tokio::fs::try_exists`.

**Fix**: single query `SELECT m.relative_path, m.mime_type, m.filename,
COALESCE(m.checksum,''), COALESCE(m.file_modified_at,''), w.path FROM media_items m
LEFT JOIN watched_folders w ON w.id = m.folder_id WHERE m.id = ?1`, one
async existence check, done. (The JSON fallback disappears with K1/D1.)

### 🔵 P7 · Stats endpoint runs 4 table scans

**File**: `backend/src/routes/stats.rs:50-91`

`COUNT(*)`, `SUM(file_size)`, `GROUP BY mime_type`, and `MAX(indexed_at)` are
four separate passes over `media_items`. The first three combine into one:

```sql
SELECT COUNT(*), COALESCE(SUM(file_size),0) FROM media_items;
```

with `MAX(indexed_at)` folded into the same select. This matters because the
frontend **polls** this endpoint (see R4).

### 🔵 P8 · `remove_deleted_items` re-stats the whole disk on every index run

**File**: `backend/src/indexer/mod.rs:384-422`

The scan in the same run already produced the complete on-disk file list
(`all_files`), but cleanup ignores it and instead stats the disk once **per DB
row** (1M `exists()` calls) with per-row `DELETE`s outside a transaction — and
it's synchronous blocking I/O inside the async `full_index`.

**Fix**: build a `HashSet` of scanned `(folder_id, relative_path)` pairs and
diff in memory; wrap the deletes in one transaction. Pass the scanned set into
the function instead of re-deriving existence from disk.

---

## 4. Resource load — the app should be almost invisible

### 🔴 R1 · Thumbnail lock map grows forever

**File**: `backend/src/thumbnails/cache.rs:130-146`

`static LOCKS: DashMap<String, Arc<Mutex<()>>>` gets one entry per unique
`checksum[:16]_width` key and **nothing ever removes entries**. For a 1M-file
library viewed at a couple of widths, that's millions of retained
`String + Arc<Mutex>` entries — plausibly hundreds of MB of RAM in a map whose
entries are needed only while a generation is in flight.

**Fix options** (simplest first):
1. Shrink the key: lock on `checksum[:16]` only (not per-width) — fewer entries,
   still correct.
2. Evict after use: replace `Arc<Mutex<()>>` values with `Weak<Mutex<()>>`;
   after releasing, try `remove(key, …)` if the strong count is 1. The
   `dashmap` entry API supports conditional removal.
3. Bounded LRU of locks (overkill here).

### 🔴 R2 · A second, never-cleaned thumbnail cache lives in `/tmp`

**Files**: `backend/src/thumbnails/image.rs:100-108` (`thumbnail_output_path` →
`std::env::temp_dir()`), `backend/src/thumbnails/cache.rs:250-254`

Every generation first writes a WebP to `/tmp/{sha256(path:width)}.webp` and
that file is **never deleted**. The cache layer then copies it into the
content-addressed cache. Consequences:

- Disk usage grows a second time, unbounded, in `/tmp` (or silently until
  reboot on systemd machines with tmpfs — where it's RAM instead).
- Every thumbnail costs an extra write + read + copy round trip.

The temp file exists only to make the final rename atomic — but the cache layer
already has its own atomic pattern (`{key}.tmp` + `rename`, cache.rs:252-254).

**Fix**: generate directly to `cache_dir/{key}.tmp` and rename onto
`{key}.webp`; delete `thumbnail_output_path` and the path-keyed temp cache
entirely. (Note the path-keyed `/tmp` cache is also **wrong** as a cache: it's
keyed by path, not content, so renamed files regenerate and moved files alias.)

### 🟡 R3 · Full cache-directory scan after every cache miss

**File**: `backend/src/thumbnails/cache.rs:256-265`

In addition to the 5-minute background timer (good), every single thumbnail
generation spawns a fire-and-forget `evict_if_needed`, which calls
`dir_size()` — a **complete `read_dir` + stat of every cached file**. With a
large cache (100K+ files) and a burst of misses (e.g., first run of a new
library), that's many concurrent full-directory scans competing with the
generations that triggered them.

**Fix**: delete the inline spawn — the timer covers eviction. If burst
protection is wanted, make it probabilistic (`if rand::<u8>() == 0`) or
counter-based (every 128th generation).

### 🟡 R4 · Frontend polls `/stats` every 5 s; backend answers with 4 scans

**Files**: `frontend/src/components/config/config-panel.tsx:67-71` and P7

While the settings panel is open, the app performs 4 full `media_items` scans
every 5 seconds — permanent background I/O and wakeups on a 1M-row database,
for a panel the user glances at occasionally.

**Fix**: poll at 15–30 s *while indexing is active* and stop when
`indexing.status` is idle; or better, drive stats refresh from the existing SSE
stream (`indexing_complete` is already broadcast) and drop the interval
entirely. Combine with the P7 query consolidation.

### 🟡 R5 · Per-thumbnail-request DB write of an unread column

**File**: `backend/src/routes/media/thumbnail.rs:75-86`, `backend/src/db/schema.rs:15`

Every served thumbnail (cache hit or miss) runs `UPDATE media_items SET
thumbnail_path = …`. Nothing ever reads `thumbnail_path` — the update exists,
per its own comment, "so the column is no longer dead". A 100-thumbnail grid
page therefore causes 100 pooled connections + WAL writes for data no one
consumes.

**Fix**: delete the write and the column (or, if it's meant as a fast-path
cache, actually **read** it first and serve from it). Removing is the KISS
option.

### 🟡 R6 · Request logging doubles log volume on gallery pages

**File**: `backend/src/middleware/logging.rs:64-68`

`LogOnRequest` prints an extra `→ request` line for every request, in addition
to the `← response` line — and a grid load is ~100 thumbnail + file requests.
That's ~200 log lines per page view for an app meant to be quiet. The request
ID generated in the span is not returned to clients, so the extra line adds
little correlation value.

**Fix**: drop `on_request` (the response line with duration is the useful one)
or demote it to `DEBUG`.

### 🔵 R7 · Runtime sizing hard-coded

**File**: `backend/src/main.rs:33`

`#[tokio::main(flavor = "multi_thread", worker_threads = 4)]` pins the runtime
to 4 workers regardless of the machine — oversubscribing a 2-core laptop,
wasting 12 cores on a 16-core desktop. The default (`worker_threads` omitted →
number of CPUs) is the right adaptive choice; combine with P1's parallel
indexing so extra cores actually get used.

### 🔵 R8 · Per-file-watcher-event config reload

**File**: `backend/src/watcher/handler.rs:186-199`

Each deletion event opens a pooled connection and reloads the watched-folder
config; a delete burst of N files performs N identical queries. Load once per
batch in `run_event_handler` and pass it down (folders change rarely).

### 🔵 R9 · Permissive CORS on a localhost API

**File**: `backend/src/main.rs:211`

`CorsLayer::permissive()` lets any website the user visits call
`http://127.0.0.1:3001` and read responses — including triggering thumbnail
generation (CPU) and enumerating the library. Since the frontend is same-origin
in production, the permissive CORS is only needed for the Vite dev server.
**Fix**: restrict allowed origins to `http://localhost:5173` (configurable via
env for dev), or omit the layer entirely in release builds.

### 🔵 R10 · Thumbnail decoding memory spikes

**File**: `backend/src/thumbnails/image.rs:85-87`

`image::open` fully decodes the source (a 4K×4K PNG ≈ 64 MB RGBA) before
Lanczos3 resize — with 4 concurrent generations, 200–400 MB transient spikes
are normal during a cold grid. If this matters on target machines: prefer
`ImageReader` + `thumbnail()` (single-pass, avoids the intermediate full-size
buffer) and consider capping decode size for absurdly large sources.

---

## Positive Observations

- ✅ **Streaming everywhere it matters**: file serving, range requests, and
  thumbnails all stream via `ReaderStream` — never loading full media into memory.
- ✅ **The watcher pipeline refactor landed well** (`watcher/stages/`): the
  handler is now an orchestrator with a clean extract → store → broadcast split
  and pure helpers with focused tests.
- ✅ **`use-sse-grid-updates.ts` is exemplary**: 50 ms batching, time-based
  event pruning, and infinite-query truncation before invalidation (avoids
  refetching every accumulated page after a reindex).
- ✅ **Ref-based zoom/pan in the image viewer** eliminates React
  reconciliation on mousemove/wheel; React state syncs are debounced.
- ✅ **Conditional view mounting** (`thumbnail-grid.tsx`) cleanly replaced the
  old `enabled`-flag approach — only the active mode's query observer exists.
- ✅ Solid SQLite hygiene: WAL, busy timeout, batched transactions, an index
  that exactly matches the cursor pagination (`file_created_at DESC, id`), and
  per-key mutexing that prevents duplicate thumbnail generation.
- ✅ Consistent validation at the route boundary with structured error bodies.

---

## Recommended Order of Work

| Phase | Items | Why first | Est. |
|-------|-------|-----------|------|
| 1 | P1 (incremental + parallel startup index), R2 (kill `/tmp` cache) | Biggest user-visible wins; startup time and silent disk growth | 4–6 h |
| 2 | K1/D1 (single folder source of truth), P2 (`.mov`) | Removes a whole bug class; unblocks P6 simplification | 3–4 h |
| 3 | R1 (lock map eviction), R5 (thumbnail_path), R3 (inline eviction), P5 (semaphore on miss) | Bounds memory & write load of the hot thumbnail path | 2–3 h |
| 4 | D7 (App.tsx duplicate hooks), P3 (MultiCollector), P4 (count cache) | Correctness + halved search cost | 2–3 h |
| 5 | D2–D6, K2–K4, K6–K7, D8, P6–P8, R4, R6–R10 | Hygiene batch, each small and independent | 6–8 h |

Each item should follow the project's TDD workflow (failing test → minimal
implementation → refactor → `cargo test` / `npm test`) and keep
`cargo clippy -- -D warnings` and `npm run lint` green.

---

## Verification Checklist (after fixes)

```bash
# Backend
cd backend && cargo test && cargo clippy -- -D warnings && cargo fmt --check

# Frontend
cd frontend && npm test && npm run typecheck && npm run lint

# Manual smoke
./scripts/dev.sh
#  - Restart with an existing large library: startup should NOT re-hash unchanged files
#  - Watch /tmp (or XDG temp): no accumulating *.webp files
#  - RSS after browsing a large grid: lock map + caches bounded
#  - Settings panel open: no 5s-interval query storm in the network tab
#  - Search with sort=score: arrow-key order matches grid order
```
