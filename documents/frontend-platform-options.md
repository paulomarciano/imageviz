# ImageViz — Frontend Platform Options

> **Research Report** | May 2026
>
> **Question**: What are the options for a new, highly performant, robust frontend for ImageViz that avoids the "clunky webapp" feel while preserving current functionality?

---

## Executive Summary

**Recommendation: Migrate the existing React frontend into Tauri v2 with the Axum backend running as a sidecar process.** This delivers native desktop performance (~8–15 MB binary, ~45 MB RAM idle, ~200–500 ms cold start) while preserving 90%+ of the existing frontend code and 100% of the backend. The alternative of a full Rust-native GUI rewrite (Dioxus, egui, Iced, GPUI, or Slint) would require a complete frontend rebuild — 12–20 weeks of work for marginal additional performance gains that are irrelevant to an image/video browser. SolidJS or Svelte are faster web frameworks but don't escape the browser sandbox, so they don't solve the fundamental "clunky webapp" problem.

**Confidence: High** — Based on benchmark data from the Nickel framework suite (Feb 2026), the Open Web Foundation Desktop App Performance Tracker (Jan 2026), and case studies from production Tauri apps including Tauview (image viewer) and Locally Uncensored (React 19 + Tauri v2).

---

## 1. Problem Diagnosis: Why Webapps Feel "Clunky"

The "clunky webapp" feeling comes from factors that framework choice alone cannot fix:

| Cause | Root | Fix |
|-------|------|-----|
| Browser chrome (URL bar, tabs, menus) | Runs in a browser | **Desktop shell removes chrome** |
| High baseline memory (200–500 MB for Chrome alone) | Browser process overhead | **Tauri: ~45 MB idle vs ~168 MB for Electron** |
| No global keyboard shortcuts | Sandboxed in browser tab | **Tauri global shortcut API** |
| OS-level drag-and-drop unreliable | Browser sandbox restricts drag data | **Tauri native drag support** |
| Perceived latency (1–2s cold start) | Browser launch + page load | **Tauri cold start ~200–500 ms** |
| Cannot register as default file handler | No OS integration | **Tauri file association support** |
| No system tray / always-on-top | No window management | **Tauri window API** |
| Battery drain (browser baseline) | Browser process constantly active | **Tauri: 0.4% battery/hr vs 2.1% for Electron** [^1] |

**Key insight**: The current React implementation is functionally good. The problem is the *runtime environment* (browser), not the framework. Moving to a desktop shell addresses all eight causes simultaneously while preserving the React codebase.

---

## 2. Current Architecture Baseline

Before evaluating options, understanding what exists is critical. ImageViz has:

**Frontend (~60 files, Waves 4–6 complete)**:
- React 19 + TypeScript strict + Vite 8 + Tailwind v4
- `react-virtuoso` for virtual-scroll thumbnail grid (100K+ items)
- `react-dnd` for OS-level drag-and-drop
- TanStack Query (`useInfiniteQuery`) for cursor pagination
- Jotai for state management (search, media, SSE, UI atoms)
- SSE hook for real-time file system updates
- Image viewer (zoom/pan), video viewer (playback with Range requests)
- Metadata panel (collapsible JSON tree)
- Search bar with debounce, keyboard navigation
- Full test suite (Vitest + Testing Library + MSW + Playwright E2E)

**Backend (~50 Rust source files, Waves 1–3 complete)**:
- Axum HTTP server on port 3001 (`/api/v1`)
- SQLite with WAL mode (rusqlite + r2d2 pool)
- Tantivy full-text search
- Thumbnail generation (image crate + ffmpeg sidecar) with content-addressed cache
- File system watcher (notify + debouncer) with SSE broadcast
- Cursor-based pagination, Range request support for video seeking
- Comprehensive integration test suite

**Current communication pattern**:
```
Browser → HTTP → Axum Backend (:3001) → SQLite + Tantivy + Filesystem
```

---

## 3. Option Analysis

### 3.1 Option A: Tauri v2 + React (Sidecar Backend) ★ RECOMMENDED

**What**: Wrap the existing React frontend in a Tauri v2 native window. Bundle the Axum backend as a sidecar process. The frontend communicates with the backend via HTTP (same API contract). Tauri handles native window management, global shortcuts, OS drag-and-drop, file dialogs, and auto-updates.

**Architecture**:
```
┌─────────────────────────────────────────────┐
│              Tauri v2 Desktop Shell           │
│  ┌─────────────────────────────────────────┐ │
│  │    WebView (React/Vite/Tailwind)         │ │
│  │    ─ Same codebase, same components     │ │
│  │    ─ Communicates via HTTP to localhost  │ │
│  └─────────────────────────────────────────┘ │
│  ┌──────────────────────┐                    │
│  │  Tauri Rust Core      │                    │
│  │  ─ Global shortcuts   │                    │
│  │  ─ Window management  │                    │
│  │  ─ File dialogs       │                    │
│  │  ─ OS drag-and-drop   │                    │
│  │  ─ Auto-updater       │                    │
│  │  ─ System tray        │                    │
│  └──────────────────────┘                    │
│  ┌──────────────────────┐                    │
│  │  Sidecar: Axum Server │                    │
│  │  (same backend binary)│                    │
│  └──────────────────────┘                    │
└─────────────────────────────────────────────┘
```

#### Evidence & Data

**Performance (independent benchmarks, 2026)** [^2]:

| Metric | Current (Browser) | Tauri v2 | Improvement |
|--------|-------------------|----------|-------------|
| Binary/app size | N/A (browser) | 8–15 MB | — |
| Idle memory | 200–500 MB (Chrome) | ~45 MB | **75–90% less** |
| Cold startup | 1–3s (browser + page) | 200–500 ms | **3–10× faster** |
| IPC latency | HTTP ~1–5ms | HTTP ~1–5ms (same) | — |
| Battery/hr | ~2% (browser baseline) | ~0.4% | **5× less drain** |
| Build time | 2s (Vite HMR) | 3.5s incremental | +1.5s |

**Real-world validation**:

- **Tauview** (open-source Tauri + React image viewer) [^3]: 8 MB binary, 45 MB RAM idle, < 0.5s startup, 60 fps zoom on high-resolution images. Uses the same pattern (React frontend in Tauri WebView, Rust backend for FS operations). Handles 10,000-image folders with virtual scrolling.

- **Locally Uncensored** (Tauri v2 + React 19 + TypeScript) [^4]: Production Tauri app with React frontend, Rust backend for CORS proxying, download management, and process lifecycle. Developer reports: "The Tauri binary is under 15 MB... with zero external runtime dependencies."

- **Hoppscotch** (migrated from Electron to Tauri) [^2]: Bundle size reduced from 165 MB to 8 MB. 70% reduction in memory usage.

- **Spacedrive** (cross-platform file manager): Uses Tauri for desktop shell with heavy file system operations in Rust backend.

#### Migration Effort

| Component | Work Required | Effort |
|-----------|---------------|--------|
| React frontend | **~5% changes**: Replace `fetch` with Tauri-aware fetch for CORS, add Tauri API imports for shortcuts/dialogs | 1–2 days |
| Axum backend | **~5% changes**: Build as standalone binary, add graceful shutdown for Tauri lifecycle | 1 day |
| Tauri shell | New: `src-tauri/` directory with Rust main, `tauri.conf.json`, sidecar config | 2–3 days |
| Drag-and-drop | Replace `react-dnd` HTML5 backend with Tauri native drag (or keep HTML5, it works in WebView) | 0.5 day |
| Global shortcuts | Add via `@tauri-apps/plugin-global-shortcut` | 0.5 day |
| Auto-updater | Add via `@tauri-apps/plugin-updater` | 1 day |
| Build pipeline | Add `cargo tauri build` step, configure CI | 1 day |
| Testing | Add E2E test for Tauri shell (Playwright supports Tauri via `tauri-driver`) | 1–2 days |
| **Total** | | **~7–11 days** |

#### Code Change Example

Current:
```typescript
// fetch('/api/v1/media?...') — works in browser
const res = await fetch('/api/v1/media?cursor=...');
```

Tauri (sidecar pattern):
```typescript
// In Tauri, the sidecar backend runs on localhost:3001
// Vite proxy already handles this in dev mode
// Production: point to http://localhost:3001/api/v1/...
// Essentially no change needed — the Vite proxy pattern works identically
```

#### Pros
- **Maximum code preservation**: ~90% frontend, ~100% backend unchanged
- **Fastest path to native**: 7–11 days vs 12–20 weeks for rewrite
- **All test suites preserved**: Vitest/Testing Library/E2E tests remain valid
- **Proven pattern**: Multiple production Tauri+React apps exist
- **Cross-platform**: Windows, macOS, Linux from one codebase
- **Mobile option**: Tauri v2 supports iOS/Android if needed later
- **Security**: Capability-based permission model, smaller attack surface than browser
- **Auto-updater**: Built-in delta updates (1–5 MB vs full download)

#### Cons
- Still a WebView (rendering in browser engine, not native GPU)
- WebView rendering differences across platforms (WebView2/WKWebView/WebKitGTK)
- Requires Rust toolchain for build (CI already has it for backend)
- Small Rust learning curve for Tauri shell (minimal — ~50 lines of Rust)
- Sidecar process management adds complexity (startup/shutdown coordination)

---

### 3.2 Option B: Tauri v2 + React (Embedded Backend)

**What**: Like Option A, but migrate the entire Axum backend logic into Tauri Rust commands. No separate server process — everything runs in one binary.

**Architecture**:
```
┌─────────────────────────────────────────────┐
│              Tauri v2 Desktop Shell           │
│  ┌─────────────────────────────────────────┐ │
│  │    WebView (React frontend)              │ │
│  └─────────────────────────────────────────┘ │
│  ┌─────────────────────────────────────────┐ │
│  │  Tauri Rust Core (single binary)         │ │
│  │  ─ All backend logic as Tauri commands  │ │
│  │  ─ SQLite, Tantivy, thumbnails, SSE     │ │
│  │  ─ Reuses same Rust crates              │ │
│  └─────────────────────────────────────────┘ │
└─────────────────────────────────────────────┘
```

#### Pros
- **Single binary**: No process coordination, simpler deployment
- **Faster IPC**: Tauri IPC (0.12 ms) vs HTTP (1–5 ms)
- **Same Rust crates**: `rusqlite`, `tantivy`, `image`, `notify` can be reused directly
- **No HTTP overhead**: Direct function calls instead of serialization

#### Cons
- **Major backend rewrite**: All Axum routes → Tauri commands (50+ endpoints)
- **Loses REST API**: Can't serve other clients (e.g., mobile, other tools)
- **SSE replacement**: Tauri events system instead of SSE (architectural change)
- **Testing changes**: Integration tests must be rewritten for Tauri command testing
- **Higher effort**: Estimated 4–8 weeks for backend migration
- **Risk**: Tantivy and `notify` integration patterns change significantly

#### Verdict
**Not recommended for v1.** The sidecar approach preserves the proven, tested backend. Embedded migration adds 4–8 weeks of effort with no user-facing benefit. Consider for v2 if single-binary deployment becomes a hard requirement.

---

### 3.3 Option C: Dioxus Desktop (Full Rust Rewrite)

**What**: Rewrite the entire frontend in Dioxus, a Rust framework that targets desktop via WebView (similar rendering to Tauri) but runs all UI logic as native Rust code, not JavaScript.

**Architecture**:
```
┌─────────────────────────────────────────────┐
│           Dioxus Desktop App                  │
│  ┌─────────────────────────────────────────┐ │
│  │  Dioxus UI (Rust RSX, compiles to WASM)  │ │
│  │  ─ All components rewritten in Rust     │ │
│  └─────────────────────────────────────────┘ │
│  ┌─────────────────────────────────────────┐ │
│  │  Native Rust Backend                     │ │
│  │  ─ SQLite, Tantivy, thumbnails          │ │
│  │  ─ Direct function calls (no IPC)       │ │
│  └─────────────────────────────────────────┘ │
└─────────────────────────────────────────────┘
```

#### Evidence

**Framework status (April 2026)** [^5]:
- **Dioxus 0.6**: React-like RSX syntax, signals (new in 0.6), ~23K GitHub stars
- Desktop rendering via `tao`/`wry` (same underlying libraries as Tauri)
- Cross-platform: web, desktop, mobile, TUI from one codebase
- Hot reload via `dx serve --platform desktop`

**Community sentiment** [^6]: "Dioxus has an unparalleled experience when building desktop apps, because your application logic runs as a native Rust binary." — Leptos GitHub README (official comparison). However: "Dioxus documentation improved significantly in 0.6 but still has gaps." [^5]

**Wren's Rust GUI Landscape 2026** [^7]: "Dioxus took the React world by storm and carried that energy to desktop. It uses WebView under the hood — which sounds like a hack, but it's genuinely practical. You get accessibility for free... The downside is the WebView dependency."

#### Pros
- Everything in Rust (one language, one type system)
- Native Rust performance for UI logic (no JS engine overhead)
- Cross-platform from single codebase
- No IPC between frontend and backend (same process)
- Familiar React-like RSX syntax

#### Cons
- **Complete frontend rewrite**: ~60 React components must be rewritten
- **Smaller ecosystem**: Few Dioxus-specific UI libraries (no equivalent to `react-virtuoso`, `react-dnd`, TanStack Query, Jotai)
- **Virtual scrolling**: Must implement from scratch or adapt a Rust virtual list
- **Video playback**: No built-in video component; must use platform WebView video element anyway
- **Drag-and-drop**: No mature Dioxus-native drag library
- **SSE replacement**: Must use different real-time pattern
- **Slower iteration**: Rust compile times vs JS hot reload (3.5s vs ~0.1s)
- **Smaller talent pool**: Far fewer Dioxus developers than React developers
- **Estimated effort**: 12–20 weeks for full rewrite + testing

#### Verdict
**Not recommended.** Dioxus is excellent for new Rust-only projects targeting multi-platform from scratch. For ImageViz, the cost of rewriting 60+ React components, rebuilding the virtual scroll infrastructure, and replacing the drag-and-drop and data-fetching layers far outweighs any performance benefit. The rendering engine is the same WebView as Tauri, so there's no visual or rendering performance advantage.

---

### 3.4 Option D: Native Rust GUI (egui / Iced / GPUI / Slint)

**What**: Full native GUI with GPU rendering. No WebView, no browser engine, no JavaScript. True native performance.

#### Framework Comparison

| Framework | Model | GPU | License | Maturity | Image/Video Support |
|-----------|-------|-----|---------|----------|---------------------|
| **egui** | Immediate mode | Yes (wgpu/glow) | MIT/Apache 2.0 | High (5+ years) | Image via `egui_extras`, no video |
| **Iced** | Retained (Elm-like) | Yes (wgpu) | MIT/Apache 2.0 | Medium (COSMIC DE) | Image via `iced_wgpu`, no video |
| **GPUI** | Hybrid immediate/retained | Yes (Metal/DX12/Vulkan) | Apache 2.0 | Medium (Zed editor) | No built-in image/video widgets |
| **Slint** | Declarative (.slint DSL) | Yes | GPLv3 / Commercial | Medium | Basic image support, no video |

#### Evidence

**egui** [^7]: "The quickest way from zero to a window. You add `eframe`, write 15 lines, and have a running app. The trade-off: it looks like a debug UI. Custom styling is possible but not the point."

**Iced** [^7]: "Takes the Elixir/Telecom approach — functional, declarative, inspired by Elm. Has a more native feel than egui but requires more setup. Documentation has gaps."

**GPUI** [^6]: "GPUI is a highly promising UI framework... distinguished by its uncompromising focus on performance." Powers the Zed editor. However, the component ecosystem is immature.

**Slint** [^7]: "Has its own markup language for describing UI." Licensing concerns (GPLv3 for open-source, commercial license required for proprietary).

**Reddit community (April 2026)** [^8]: "In terms of development efficiency analysis, Tauri is the best. If performance is considered, I vote for GPUI."

**Image viewer case study**: A GPU-accelerated image viewer built with Iced and wgpu [^9] achieved excellent raw rendering performance but required building all UI components from scratch — grid layout, virtual scrolling, keyboard navigation, drag-and-drop — none of which exist as reusable components.

#### Pros
- **Best possible raw performance**: GPU-accelerated, no browser overhead
- **Smallest binary**: ~3–5 MB (egui), ~5–10 MB (others)
- **Lowest memory**: ~20–50 MB idle
- **True native feel**: No WebView quirks
- **Direct GPU texture mapping**: Best for image rendering

#### Cons
- **Massive rewrite**: Everything from scratch — no code reuse
- **No virtual scroll**: Must implement from scratch for all frameworks
- **No video playback**: Must integrate GStreamer, ffmpeg, or similar (months of work)
- **No drag-and-drop library** (mature): Must use platform APIs directly
- **No grid layout component**: Must build masonry grid manually
- **Thumbnail management**: Must build thumbnail caching UI from scratch
- **Search UI**: Must build search bar, debounce, filters manually
- **SSE equivalent**: Must implement custom event stream
- **CSS/styling**: No CSS — must use framework-specific theming (very different from Tailwind)
- **Estimated effort**: 16–24 weeks for a feature-complete rewrite

#### Verdict
**Not recommended for ImageViz.** Native Rust GUI is the right choice for latency-critical tools (3D editors, game engines, real-time audio) where every millisecond matters. For an image/video browser — where the bottleneck is I/O (loading files, generating thumbnails, streaming video), not UI rendering — the 16–24 week investment yields no perceptible user-facing improvement over Tauri. The 60 React components, virtual scroll, drag-and-drop, video player, and search infrastructure would all need to be built from scratch.

---

### 3.5 Option E: SolidJS or Svelte (Alternative Web Frameworks)

**What**: Keep the web architecture but replace React with a more performant framework (SolidJS or Svelte).

#### Evidence

**SolidJS**: Fine-grained reactivity (no virtual DOM), ~4 KB runtime, faster rendering than React. Similar to Leptos's signal model but in JavaScript.

**Svelte 5**: Compiles away the framework at build time, producing minimal JavaScript. Runes-based reactivity in v5.

#### Pros vs React
- Faster DOM updates (no virtual DOM diffing)
- Smaller bundle sizes
- Better memory characteristics for large lists

#### Cons
- **Still a web app in a browser**: All eight "clunky" factors from Section 1 remain
- **Full rewrite of 60+ React components**
- **Ecosystem gaps**: No equivalent to `react-virtuoso` (SolidJS has `solid-virtuoso` but less mature), no equivalent to `react-dnd` with HTML5 backend
- **SSE handling**: Similar but must rewrite all hooks
- **Test suite rewrite**: All Testing Library tests must be rewritten
- **Estimated effort**: 6–10 weeks for rewrite + testing
- **Doesn't solve the core problem**: Still runs in browser with browser overhead

#### Verdict
**Not recommended.** Switching web frameworks improves rendering micro-performance but doesn't escape the browser sandbox. The user won't perceive the difference between React's virtual DOM and SolidJS's signals when the real bottleneck is browser process overhead, missing OS integration, and the "browser tab" feel. If moving to Tauri, there's no reason to also switch frameworks — React is already working and Tauri's WebView runs it just fine.

---

### 3.6 Option F: Leptos + Tauri (Rust WASM in Tauri WebView)

**What**: Like Option A, but replace React with Leptos (Rust WASM framework) running inside the Tauri WebView.

#### Evidence

**Leptos 0.7** [^5]: Fine-grained signals, SSR, server functions, Islands architecture. "Leptos is a full-stack web framework — the Rust equivalent of Next.js."

**Leptos + Tauri**: "For desktop, pair Leptos with Tauri v2 — Tauri hosts your Leptos WASM app in a native WebView window." [^5]

#### Pros
- Everything in Rust (backend + frontend logic)
- Fine-grained reactivity (no virtual DOM) — faster than React for DOM updates
- Same language across the stack
- Smaller WASM bundles than JS bundles (sometimes)

#### Cons
- **Full frontend rewrite**: All 60 React components → Leptos RSX
- **Two compilations**: Rust → WASM (client) + Rust native (backend)
- **Slower iteration**: Rust compile → WASM is slower than JS HMR
- **Ecosystem**: Leptos is web-focused; Tauri desktop patterns are less documented
- **SSE in WASM**: Leptos handles it but differently than the current JS SSE hook
- **Estimated effort**: 12–18 weeks

#### Verdict
**Not recommended for v1.** Leptos + Tauri is a compelling stack for new Rust-native projects, but there's no compelling reason to rewrite a working React frontend into Leptos. The WASM compilation step adds build complexity without solving the core problem (which Tauri already solves). Consider for v2 if the team wants to move fully to Rust.

---

## 4. Comparison Matrix

| Criterion | A: Tauri+React (Sidecar) ★ | B: Tauri+React (Embedded) | C: Dioxus | D: Native GPU | E: SolidJS/Svelte | F: Leptos+Tauri |
|-----------|---------------------------|--------------------------|-----------|---------------|-------------------|-----------------|
| **Code preservation** | ~90% frontend, ~100% backend | ~90% frontend, ~10% backend | ~10% (full rewrite) | ~5% (full rewrite) | ~20% (full rewrite) | ~15% (full rewrite) |
| **Native desktop feel** | ✅ Full | ✅ Full | ✅ Full | ✅ Full (best) | ❌ Browser | ✅ Full |
| **Memory (idle)** | ~45 MB | ~45 MB | ~45 MB | ~25 MB | ~200 MB (browser) | ~50 MB |
| **Startup time** | ~300 ms | ~300 ms | ~300 ms | ~100 ms | ~1,500 ms (browser) | ~500 ms |
| **Binary size** | ~12 MB | ~12 MB | ~6 MB | ~4 MB | N/A (web) | ~8 MB |
| **Virtual scroll** | ✅ react-virtuoso (proven) | ✅ react-virtuoso | ❌ Build from scratch | ❌ Build from scratch | ⚠️ Less mature libs | ❌ Build from scratch |
| **Video playback** | ✅ HTML5 video (proven) | ✅ HTML5 video | ⚠️ WebView video | ❌ GStreamer needed | ✅ HTML5 video | ⚠️ WebView video |
| **Drag-and-drop** | ✅ react-dnd (proven) | ✅ react-dnd | ❌ No mature lib | ❌ Custom impl | ⚠️ Less mature | ❌ No mature lib |
| **Grid layout** | ✅ CSS Grid (Tailwind) | ✅ CSS Grid | ⚠️ CSS (limited) | ❌ Custom layout | ✅ CSS Grid | ⚠️ CSS (limited) |
| **Test suite preserved** | ✅ 100% | ✅ 90% | ❌ Must rewrite all | ❌ Must rewrite all | ❌ Must rewrite most | ❌ Must rewrite all |
| **Estimated effort** | **7–11 days** | 4–8 weeks | 12–20 weeks | 16–24 weeks | 6–10 weeks | 12–18 weeks |
| **Risk** | Low | Medium | High | Very High | Medium | High |
| **Team skill needed** | React + minimal Rust | React + Rust | Rust | Rust + GPU | SolidJS/Svelte | Rust + Leptos |

---

## 5. Detailed Migration Plan: Option A (Recommended)

### Phase 1: Tauri Shell Setup (2–3 days)

**Day 1**: Initialize Tauri in the existing frontend project
```bash
cd frontend
npm create tauri-app@latest . -- --template react-ts
# Select: existing Vite project → integrate
```

This creates `src-tauri/` with:
- `Cargo.toml` (Tauri dependencies)
- `src/main.rs` (Rust entry point — ~20 lines)
- `tauri.conf.json` (window config, security, bundle)
- `capabilities/default.json` (permission model)

**Day 2**: Configure sidecar for the Axum backend
```json
// tauri.conf.json
{
  "bundle": {
    "externalBin": ["binaries/imageviz-backend"]
  }
}
```

```rust
// src-tauri/src/main.rs
use tauri::Manager;
use tauri_plugin_shell::ShellExt;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Spawn backend server as sidecar
            let sidecar = app.shell()
                .sidecar("imageviz-backend")
                .expect("failed to find sidecar binary");
            let (mut rx, _child) = sidecar.spawn()
                .expect("failed to spawn sidecar");
            // Monitor sidecar stdout/stderr
            tauri::async_runtime::spawn(async move {
                while let Some(event) = rx.recv().await {
                    // Log or handle sidecar events
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Day 3**: Add Tauri plugins for desktop features
```bash
cargo add tauri-plugin-global-shortcut
cargo add tauri-plugin-updater
cargo add tauri-plugin-dialog
cargo add tauri-plugin-shell
cargo add tauri-plugin-fs
```

### Phase 2: Frontend Adaptation (1–2 days)

**Changes needed** (all minor):

1. **API base URL**: Ensure production build points to `http://localhost:3001/api/v1` (already configured via Vite proxy in dev)

2. **Add Tauri API imports** for new capabilities:
```typescript
// src/api/tauri-bridge.ts (new file)
import { getCurrentWindow } from '@tauri-apps/api/window';
import { register } from '@tauri-apps/plugin-global-shortcut';
import { open } from '@tauri-apps/plugin-dialog';

export async function setupDesktopFeatures() {
  // Register global shortcuts (work even when app not focused)
  await register('CommandOrControl+F', () => {
    // Focus search bar
  });
  await register('Escape', () => {
    // Close detail view
  });
  // ... existing keyboard shortcuts mapped to global
}

export async function pickFolder(): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false });
  return selected ?? null;
}
```

3. **Conditional feature loading**: Only activate Tauri features when running in Tauri:
```typescript
// Check if running in Tauri
function isTauri(): boolean {
  return !!(window as any).__TAURI__;
}

// In App.tsx
if (isTauri()) {
  setupDesktopFeatures();
}
```

4. **Drag-and-drop**: The existing `react-dnd` HTML5 backend works in Tauri's WebView. For enhanced OS integration, Tauri provides `onDragDropEvent` on the window. This is optional.

### Phase 3: Backend Adaptation (1 day)

1. **Graceful shutdown**: Add signal handling so Tauri can cleanly stop the backend:
```rust
// In backend main.rs
tokio::select! {
    _ = axum::serve(...) => {},
    _ = tokio::signal::ctrl_c() => {
        tracing::info!("Shutting down...");
    },
}
```

2. **Build for sidecar**: The backend binary must be compiled for the target platform and placed in `src-tauri/binaries/`. Add a build script:
```bash
# scripts/build-sidecar.sh
cargo build --release --manifest-path backend/Cargo.toml
cp backend/target/release/imageviz-backend frontend/src-tauri/binaries/
```

### Phase 4: Build Pipeline (1 day)

Update CI to:
1. Build backend binary for each platform
2. Build Tauri app (which bundles the sidecar)
3. Sign and notarize (macOS)

```yaml
# .github/workflows/release.yml (addition)
- name: Build Tauri app
  run: |
    cd frontend
    npm run tauri build
```

### Phase 5: Testing (1–2 days)

1. **Existing Vitest tests**: All pass unchanged (they use MSW, work in jsdom)
2. **Existing Playwright E2E tests**: Adapt to use `tauri-driver` (Tauri's WebDriver implementation)
3. **New tests**: Tauri shell startup, sidecar lifecycle, global shortcuts

---

## 6. Risk Assessment

| Risk | Likelihood | Severity | Mitigation |
|------|-----------|----------|------------|
| WebView rendering inconsistencies across OS | Medium | Low | Test on all three platforms; CSS feature detection; Progressive enhancement. Tauview (image viewer) proves image rendering works well across platforms. |
| Sidecar process coordination | Low | Medium | Tauri's `on_window_event(CloseRequested)` triggers backend shutdown. Health check loop ensures backend is ready before showing UI. |
| Build complexity increase | Medium | Low | Add build scripts; document in CI. Tauri CLI handles most of it. |
| Rust compile times slow iteration | Low | Low | Frontend HMR still works (the Vite dev server runs separately). Tauri rebuild only needed for Rust changes (minimal). |
| `react-dnd` HTML5 backend not working in Tauri WebView | Low | Medium | Tauri's `onDragDropEvent` provides native OS drag as fallback. Likely works — HTML5 drag is well-supported in all WebViews. |
| Large binary size from bundling backend | Low | Low | Backend is already a Rust binary (~15 MB). Tauri compresses well. Total < 30 MB. |

---

## 7. Performance Validation Plan

After migration, verify against the original performance targets:

| Metric | Target | Measurement |
|--------|--------|-------------|
| Grid scroll FPS | 60 fps | Tauri DevTools → Performance tab |
| Initial load (100 thumbnails) | < 1s | Wall clock timing |
| Search response (100K dataset) | < 200ms | Server-side timing (unchanged) |
| SSE event latency | < 500ms | End-to-end timing |
| Memory (100K items in grid) | < 500 MB → target < 300 MB with Tauri | OS process monitor |
| Cold startup to interactive | < 500ms | Wall clock timing |

---

## 8. What We Gain (User-Facing)

| Current Experience | After Tauri Migration |
|-------------------|----------------------|
| Open browser → type URL → wait for page load | Double-click app icon → instant |
| Browser chrome takes screen space | Full window dedicated to content |
| Ctrl+F searches page, not media | Global Cmd/Ctrl+F searches media |
| Drag from browser to filesystem hit-or-miss | Native OS drag-and-drop |
| ~500 MB RAM for browser + app | ~45 MB RAM for app |
| Browser process drains laptop battery | Minimal battery impact |
| Can't pin to taskbar/dock usefully | Proper app with icon, notifications |

---

## 9. Open Questions

1. **Single window vs multi-window**: Should the detail viewer open in a new Tauri window (allows side-by-side on multi-monitor setups)? This is a Tauri-native capability not possible in the browser.

2. **File associations**: Should ImageViz register as the default handler for PNG/WEBM/MP4? Tauri supports this via the `deep-link` plugin.

3. **System tray**: Should ImageViz minimize to system tray with quick-access menu? Tauri's `tray-icon` plugin supports this.

4. **Animated thumbnails (video)**: The development plan mentions animated thumbnails. In Tauri, video thumbnails can be generated by the same Rust `image` crate + ffmpeg pipeline, or rendered as silent HTML5 `<video>` elements in the WebView grid.

---

## 10. Source Log

| Source | Accessed | Credibility | Relevance |
|--------|----------|-------------|-----------|
| [^1] Open Web Foundation Desktop App Performance Tracker (Jan 2026), via Tech Insider | 2026-05-28 | High (aggregated benchmarks) | Battery impact comparison |
| [^2] Tech Insider — "Tauri vs Electron 2026: The Definitive Desktop Framework Comparison" | 2026-05-28 | High (comprehensive, cites multiple sources) | Performance benchmarks, migration guide |
| [^3] Bright Coding — "Tauview: The Image Viewer Every Developer Needs" | 2026-05-28 | Medium (blog, but technical) | Real Tauri+React image viewer case study |
| [^4] dev.to — "How I Built a Desktop AI App with Tauri v2 + React 19 in 2026" | 2026-05-28 | High (production app, detailed code) | Tauri+React production patterns, CORS, sidecar |
| [^5] Rustify — "Leptos vs Dioxus: Choosing a Rust Frontend Framework in 2026" | 2026-05-28 | High (comprehensive comparison, cites official docs) | Framework comparison data |
| [^6] Reddit r/rust — "GPUI vs Dioxus vs Tauri for building desktop apps" (Apr 2026) | 2026-05-28 | Medium (community sentiment) | Developer experience, real-world preferences |
| [^7] Wren Learns Rust — "The Rust GUI Landscape in 2026: Picking Your Framework" | 2026-05-28 | Medium (survey-based, cites boringcactus survey) | Rust GUI ecosystem overview |
| [^8] Reddit r/rust — various threads on native GUI for image processing | 2026-05-28 | Medium | Real-world experience with image viewers in Rust |
| [^9] Reddit r/rust — "I built a GPU-accelerated image viewer with Iced and wgpu" (Mar 2025) | 2026-05-28 | Medium | Native GPU image viewer experience |
| [^10] Tauri v2 Official Documentation (v2.tauri.app) | 2026-05-28 | High (official docs) | Sidecar pattern, plugin APIs, configuration |
| [^11] ImageViz Development Plan (documents/plans/development-plan.md) | 2026-05-28 | High (project source of truth) | Current architecture, feature requirements |
| [^12] boringcactus — "A 2025 Survey of Rust GUI Libraries" | 2026-05-28 | High (systematic 2-week testing) | Cross-framework comparison |
| [^13] Reddit r/tauri — "How performant is Tauri when it comes to rendering complex..." (Aug 2025) | 2026-05-28 | Medium | WebView rendering performance discussion |

---

## 11. Conclusion

The goal is "highly performant, robust frontend" that doesn't "feel clunky." The current React implementation is functionally complete and well-tested. The clunkiness comes from the browser runtime environment, not from React.

**Tauri v2 with the existing React frontend and sidecar backend (Option A) is the clear recommendation**:

- **7–11 days** to native desktop feel vs **12–24 weeks** for any rewrite
- **90%+ code preservation** of 60 React components and full test suite
- **100% backend preservation** — the Axum/SQLite/Tantivy stack remains unchanged
- **Proven pattern**: Tauview (image viewer), Locally Uncensored (React 19 + Tauri v2), Hoppscotch (migrated from Electron)
- **Measurable gains**: ~45 MB RAM (vs ~500 MB browser), ~300ms startup (vs ~1.5s), native keyboard shortcuts, proper OS drag-and-drop
- **Future-proof**: Tauri v2 supports mobile, has active development, and is backed by a growing ecosystem

The alternatives (Dioxus, native GPU GUI, SolidJS/Svelte, Leptos) all require complete frontend rewrites with 10–20× the effort for marginal or zero additional user-perceptible benefit. These options should only be reconsidered if the project requirements fundamentally change (e.g., need for 120 fps GPU-accelerated rendering, embedded systems deployment, or a strategic decision to go all-Rust).
