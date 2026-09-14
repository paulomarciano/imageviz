# ImageViz — Tauri v2 Desktop Migration Plan

> **Plan** | May 2026 | Status: Planning  
> **Based on**: [Frontend Platform Options](../frontend-platform-options.md) — Option A (Recommended)  
> **Depends on**: Waves 0–7 complete (current web app fully functional)

---

## 0. Executive Summary

**Goal**: Wrap the existing React frontend in a Tauri v2 native desktop window with the Axum backend running as a sidecar process. The web app continues to work unchanged — Tauri is an _additional_ delivery mode.

**Architecture**:

```
Two Build Targets, One Codebase:

Web (existing — unchanged):
  Browser ──HTTP──▶ Vite (:5173) ──proxy──▶ Axum Backend (:3001)
                                              │
Desktop (new — Tauri v2):                     │
  ┌──────────────────────────────────────────────┐
  │  Tauri v2 Native Window                      │
  │  ┌────────────────────────────────────────┐  │
  │  │  WebView (React/Vite/Tailwind)          │  │
  │  │  ─ Same code, same components           │  │
  │  │  ─ Communicates via HTTP to :3001       │  │
  │  └────────────────────────────────────────┘  │
  │  ┌──────────────────┐  ┌──────────────────┐  │
  │  │  Tauri Rust Core  │  │  Sidecar: Axum   │  │
  │  │  ─ Shortcuts      │  │  (same backend   │  │
  │  │  ─ Window mgmt    │  │   binary)        │  │
  │  │  ─ Dialogs        │  │  :3001           │  │
  │  │  ─ Drag-and-drop  │  │                  │  │
  │  │  ─ Auto-updater   │  │                  │  │
  │  └──────────────────┘  └──────────────────┘  │
  └──────────────────────────────────────────────┘
      Binary: ~12 MB | RAM: ~45 MB | Startup: < 500 ms
```

**Total Effort**: ~7–11 days | **Code Preservation**: ~90% frontend, ~100% backend  
**Risk**: Low | **Reversible**: Yes — Tauri can be removed without affecting the web app

---

## Phases at a Glance

| Phase | Name | Days | Tests Written | Key Artifact |
|-------|------|------|---------------|--------------|
| P1 | Environment Isolation | 1 | 4–6 | Dual-mode build scripts, conditional Vite config |
| P2 | Tauri Shell Scaffold | 1–2 | 3–4 | `src-tauri/` directory, working `tauri dev` |
| P3 | Sidecar Backend | 1 | 4–5 | Axum spawned/managed by Tauri lifecycle |
| P4 | Frontend Adaptation | 1–2 | 5–7 | Conditional imports, Tauri-aware client |
| P5 | Desktop Features | 1–2 | 6–8 | Global shortcuts, native dialogs, OS drag |
| P6 | Build & Release CI | 1 | 3–5 | Multi-platform CI, sidecar bundling |
| P7 | E2E Testing & Polish | 1 | 4–6 | WebDriver tests, startup perf validation |

---

## TDD Strategy (Cross-Cutting)

Every phase follows the same TDD loop, consistent with the existing project conventions:

1. **Write a failing test** — Define acceptance criteria before implementation
2. **Implement minimum code** — Pass the test with the simplest solution
3. **Refactor** — Clean up while keeping tests green
4. **Add edge case tests** — Empty states, error paths, boundaries
5. **Verify** — Run the full test suite (`cargo test`, `npm test`) before marking complete

**Critical constraint**: The existing web test suite (Vitest + Testing Library + MSW + Playwright) must continue to pass at every commit. No regression allowed.

### Test Layer Mapping (Tauri Additions)

```
                  ╱  Tauri WebDriver E2E  ╲       ~5-8 tests (full Tauri app lifecycle)
                 ╱──────────────────────────╲
                ╱    New Integration Tests    ╲      ~8-12 tests (sidecar startup, shortcut routing, CSP)
               ╱────────────────────────────────╲
              ╱       New Unit Tests              ╲   ~12-16 tests (conditional imports, build flags, config)
             ╱──────────────────────────────────────╲
────────────────────────────────────────────────────────
         Existing Test Pyramid (Waves 0-7)          ~200+ tests (UNCHANGED)
────────────────────────────────────────────────────────
```

### Test Environment Strategy

| Environment | Web Tests (Existing) | Tauri-Only Tests (New) |
|-------------|---------------------|----------------------|
| **Vitest + jsdom** | All pass unchanged | Tauri features mocked to `isTauri: false` |
| **Vitest + jsdom + `isTauri: true`** | N/A | New: test Tauri-specific code paths |
| **Playwright (browser)** | All pass unchanged | Unchanged |
| **tauri-driver + WebDriverIO** | N/A | New: full Tauri app E2E |

### Mock Pattern for Tauri in Vitest

```typescript
// frontend/src/test-utils/mock-tauri.ts (NEW — test helper)

import { vi } from 'vitest';

/**
 * Mock @tauri-apps/api for jsdom tests.
 * @param isTauri - Set to true when testing Tauri-specific code paths.
 */
export function mockTauri(isTauri: boolean = false) {
  vi.mock('@tauri-apps/api', () => ({
    core: { isTauri },
  }));

  vi.mock('@tauri-apps/plugin-global-shortcut', () => ({
    register: vi.fn(),
    unregister: vi.fn(),
    isRegistered: vi.fn().mockResolvedValue(false),
  }));

  vi.mock('@tauri-apps/plugin-dialog', () => ({
    open: vi.fn().mockResolvedValue(null),
    save: vi.fn().mockResolvedValue(null),
    message: vi.fn(),
    ask: vi.fn().mockResolvedValue(true),
  }));

  // Window mock for non-Tauri environment
  Object.defineProperty(window, '__TAURI__', {
    value: isTauri ? {} : undefined,
    writable: true,
  });
}
```

---

## Phase 1: Environment Isolation (1 day)

**Goal**: Set up dual-mode development and build pipelines so the web app continues to work exactly as before while the Tauri shell can develop in parallel. No Tauri code yet — just infrastructure.

**Rationale**: This prevents mutating the existing `package.json`, `vite.config.ts`, or build scripts in ways that could regress the web app. The Tauri pipeline is added alongside, not on top of.

### Task 1.1: Add Tauri npm Scripts

**Files**: `frontend/package.json`

**What**: Add `tauri` script and `@tauri-apps/cli` as a dev dependency. Existing scripts (`dev`, `build`, `test`, `lint`, `typecheck`) remain untouched.

```json
{
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "preview": "vite preview",
    "lint": "eslint .",
    "format": "prettier --write .",
    "format:check": "prettier --check .",
    "test": "vitest run",
    "test:watch": "vitest",
    "typecheck": "tsc --noEmit",
    "tauri": "tauri"
  }
}
```

**Verification**: `npm run dev` still works. `npm test` still passes. `npm run build` still produces `dist/`.

### Task 1.2: Add Dual-Mode Vite Proxy Configuration

**Files**: `frontend/vite.config.ts`

**What**: The Vite proxy currently forwards `/api` to `http://localhost:3001`. In Tauri dev mode, the backend port might differ or need explicit localhost resolution. Add a conditional that always works:

```typescript
// vite.config.ts — keep existing, add conditional comment for future Tauri awareness
export default defineConfig({
  // ... existing config unchanged ...
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: process.env.VITE_BACKEND_URL || 'http://localhost:3001',
        changeOrigin: true,
      },
    },
  },
});
```

**Why `VITE_BACKEND_URL`**: When running in Tauri, the frontend might need to point at a different backend URL (same `localhost:3001` in sidecar mode, but this makes it explicit and testable). The default remains `http://localhost:3001` — no change to web dev experience.

**Verification**: `npm run dev` still proxies to `:3001`. Setting `VITE_BACKEND_URL=http://localhost:3002 npm run dev` proxies to `:3002`.

### Task 1.3: Add `VITE_TAURI` Build Flag

**Files**: `frontend/vite.config.ts`, `frontend/src/vite-env.d.ts`

**What**: Tauri's `beforeDevCommand`/`beforeBuildCommand` can set `TAURI_ENV` env var. Inject it as a Vite define for compile-time tree-shaking:

```typescript
// vite.config.ts — additions
export default defineConfig({
  define: {
    __TAURI_BUILD__: JSON.stringify(process.env.TAURI_ENV === 'tauri'),
  },
  // ... rest unchanged ...
});
```

```typescript
// frontend/src/vite-env.d.ts — additions
declare const __TAURI_BUILD__: boolean;
```

**Why a build flag**: Tauri-specific code can be gated: `if (__TAURI_BUILD__) { ... }`. Vite tree-shakes dead branches in production builds, so browser builds won't include Tauri imports. This is the _static_ check — we also need runtime detection (`core.isTauri`) for dev mode and single-binary scenarios.

**Verification**: 
- `npm run build` sets `__TAURI_BUILD__` to `false` → web build doesn't include Tauri code.
- `TAURI_ENV=tauri npm run build` sets it to `true` → desktop build includes Tauri code.

### Task 1.4: Write Environment Isolation Tests

**Files**: `frontend/src/__tests__/environment/tauri-flag.test.ts` (NEW)

**What**: Test that the build flag works correctly in both modes.

```typescript
// TDD: Write these before implementing 1.3
describe('__TAURI_BUILD__ flag', () => {
  it('is false in web mode', () => {
    expect(__TAURI_BUILD__).toBe(false);
  });

  it('can be used to conditionally exclude Tauri imports', () => {
    // If true, the following import would succeed
    // If false, it doesn't matter — tree-shaken
    expect(typeof __TAURI_BUILD__).toBe('boolean');
  });
});
```

### Task 1.5: Add `.gitignore` for Tauri Artifacts

**Files**: `.gitignore` (amend), `frontend/src-tauri/.gitkeep` (NEW)

**What**: Add `frontend/src-tauri/target/` and `frontend/src-tauri/gen/` to `.gitignore`. Commit a `.gitkeep` in `frontend/src-tauri/binaries/` and `frontend/src-tauri/icons/`.

### Phase 1 Exit Criteria

- [ ] `npm run dev` starts Vite dev server on :5173 with proxy to :3001
- [ ] `npm test` passes all existing tests
- [ ] `npm run build` produces `frontend/dist/` as before
- [ ] `__TAURI_BUILD__` flag is `false` in web builds
- [ ] `VITE_BACKEND_URL` env var changes proxy target (tested manually)
- [ ] Git ignores `src-tauri/target/` and `src-tauri/gen/`

---

## Phase 2: Tauri Shell Scaffold (1–2 days)

**Goal**: Initialize the `src-tauri/` directory with a working Tauri configuration. `npm run tauri dev` opens a native window running the React app (without the backend sidecar yet — the Vite proxy handles API calls during dev).

**Critical**: The web app must continue to `npm run dev` and `npm run build` without any change in behavior.

### Task 2.1: Install Tauri Dependencies

**Files**: `frontend/package.json`

```bash
npm install --save-dev @tauri-apps/cli@latest
npm install @tauri-apps/api@latest
```

**What**: Add the Tauri CLI (devDep) and core JS API (dep). These are _additions_ — no existing packages are removed or changed.

**Verification**: `node_modules/@tauri-apps/cli` and `node_modules/@tauri-apps/api` exist. `npm test` still passes (Tauri API is not imported yet).

### Task 2.2: Write `tauri.conf.json` for ImageViz

**Files**: `frontend/src-tauri/tauri.conf.json` (NEW)

**What**: Create the Tauri configuration reflecting ImageViz's specific needs — sidecar backend, CSP for local media, window sizing for image browsing.

```json
{
  "productName": "ImageViz",
  "version": "0.1.0",
  "identifier": "app.imageviz.desktop",
  "build": {
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build",
    "devUrl": "http://localhost:5173",
    "frontendDist": "../dist"
  },
  "app": {
    "withGlobalTauri": false,
    "security": {
      "csp": {
        "default-src": "'self'",
        "connect-src": "ipc: http://ipc.localhost http://localhost:3001 ws://localhost:3001",
        "img-src": "'self' asset: http://asset.localhost http://localhost:3001 blob: data:",
        "media-src": "'self' asset: http://localhost:3001 blob:",
        "script-src": "'self' 'wasm-unsafe-eval'",
        "style-src": "'unsafe-inline' 'self'",
        "font-src": "'self'"
      },
      "capabilities": []
    },
    "windows": [
      {
        "fullscreen": false,
        "height": 900,
        "resizable": true,
        "title": "ImageViz",
        "width": 1600,
        "minWidth": 600,
        "minHeight": 400,
        "decorations": true,
        "center": true
      }
    ]
  },
  "bundle": {
    "active": true,
    "targets": ["deb", "appimage", "dmg", "nsis"],
    "createUpdaterArtifacts": true,
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "externalBin": [
      "binaries/imageviz-backend"
    ]
  },
  "plugins": {}
}
```

**Key decisions embedded in this config**:

| Setting | Value | Rationale |
|---------|-------|-----------|
| `devUrl` | `http://localhost:5173` | Matches Vite's dev server port (from `vite.config.ts`) |
| `frontendDist` | `../dist` | Relative to `src-tauri/`; Vite outputs to `frontend/dist/` |
| `width` / `height` | 1600 × 900 | Generous default for image browsing; user can resize |
| CSP `connect-src` | includes `http://localhost:3001` | Sidecar Axum backend on port 3001 |
| CSP `img-src` / `media-src` | includes `http://localhost:3001` | Thumbnails and video served from backend |
| CSP `media-src` | includes `blob:` | Video playback may use blob URLs |
| `withGlobalTauri` | `false` | Use npm imports (preferred for TypeScript + tree-shaking) |
| Bundle targets | deb, appimage, dmg, nsis | Linux primary, macOS/Windows supported |
| `externalBin` | `binaries/imageviz-backend` | Sidecar path (configured in Phase 3) |

### Task 2.3: Generate `src-tauri/` Structure

**Files**: `frontend/src-tauri/` (NEW directory tree)

**What**: Create the Rust crate that Tauri needs. Use `npm run tauri init` (interactive) or manually create:

```
frontend/src-tauri/
├── Cargo.toml              # Tauri + plugin dependencies
├── build.rs                # tauri_build::build()
├── tauri.conf.json         # (from Task 2.2)
├── capabilities/
│   └── default.json        # Default empty capability
├── icons/                   # App icons (placeholder PNGs initially)
│   └── .gitkeep
├── binaries/
│   └── .gitkeep            # Sidecar binary goes here (Phase 3)
└── src/
    ├── main.rs             # Desktop entry point (generated, ~3 lines)
    └── lib.rs              # Rust logic — Tauri builder setup
```

**`Cargo.toml`**:

```toml
[package]
name = "imageviz-tauri"
version = "0.1.0"
edition = "2024"

[lib]
name = "imageviz_tauri_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
# Plugins added in later phases
```

**`src/lib.rs`** (minimal — just enough to open a window):

```rust
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**`src/main.rs`** (generated):

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    imageviz_tauri_lib::run()
}
```

### Task 2.4: Verify Tauri Dev Loop

**What**: Start the Axum backend on `:3001`, then run `npm run tauri dev`. The native window should open, load the React app from Vite's dev server, and the app should function identically to the browser version (API calls proxied through Vite → Axum).

**Verification checklist**:
- [ ] `npm run tauri dev` opens a native window
- [ ] The React app renders inside the Tauri window
- [ ] Thumbnail grid loads images from the backend
- [ ] Search works (queries reach the Tantivy index)
- [ ] Video preview works (Range requests)
- [ ] SSE events arrive (file watcher notifications)
- [ ] `npm run dev` (browser) still works identically
- [ ] `npm test` passes all existing tests

**Common setup issues**:
1. **Linux**: Requires `libwebkit2gtk-4.1-dev` (instructions in `AGENTS.md` CI section)
2. **Port conflict**: Ensure Axum backend is on `:3001`, Vite on `:5173`
3. **CORS**: The existing `CorsLayer::permissive()` in `backend/src/main.rs` already handles Tauri's origin

### Task 2.5: Write Tauri Shell Tests

**Files**: `frontend/src/__tests__/environment/tauri-config.test.ts` (NEW)

**What**: Test that the Tauri configuration is valid and doesn't break the web build.

```typescript
describe('Tauri shell configuration', () => {
  it('does not affect web build', () => {
    // The web build should still produce dist/index.html
    // with no Tauri artifacts
    expect(__TAURI_BUILD__).toBe(false);
  });

  it('Tauri API is not loaded in web mode', () => {
    // In web mode, isTauri should be false
    // This ensures conditional loading works
    expect((window as any).__TAURI__).toBeUndefined();
  });
});
```

### Phase 2 Exit Criteria

- [ ] `npm run tauri dev` opens ImageViz in a native window
- [ ] All existing React functionality works inside Tauri window
- [ ] `npm run dev` (browser) still works identically
- [ ] `npm test` passes all existing tests (200+ tests)
- [ ] `npm run build` produces valid `dist/`
- [ ] `.gitignore` correctly excludes Tauri build artifacts

---

## Phase 3: Sidecar Backend (1 day)

**Goal**: Bundle the Axum backend as a sidecar process managed by Tauri's lifecycle. In dev mode, the backend runs separately (no change from current). In production, Tauri spawns the backend on launch and kills it on exit.

**Why sidecar over embedded**: Preserves 100% of the backend code, keeps the proven HTTP API contract, allows the backend to be developed/tested independently, and retains the option to serve other clients (mobile, web) in the future.

### Task 3.1: Add Sidecar Build Script

**Files**: `scripts/build-sidecar.sh` (NEW)

**What**: Compile the Axum backend in release mode and copy it to the Tauri binaries directory with the correct target-triple suffix.

```bash
#!/usr/bin/env bash
# Build the Axum backend for sidecar bundling.
# Usage: ./scripts/build-sidecar.sh [target-triple]
#   ./scripts/build-sidecar.sh                          # native build
#   ./scripts/build-sidecar.sh x86_64-unknown-linux-gnu  # cross-compile

set -euo pipefail
TRIPLE="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
SUFFIX="${TRIPLE}"
SUFFIX_EXT=""

case "$TRIPLE" in
  *windows*) SUFFIX_EXT=".exe" ;;
esac

echo "Building imageviz-backend for ${TRIPLE}..."
cargo build --release --manifest-path backend/Cargo.toml --target "${TRIPLE}"

DEST="frontend/src-tauri/binaries/imageviz-backend-${SUFFIX}${SUFFIX_EXT}"
mkdir -p frontend/src-tauri/binaries
cp "backend/target/${TRIPLE}/release/imageviz-backend${SUFFIX_EXT}" "${DEST}"
echo "Copied to ${DEST}"
```

**Verification**: Run `bash scripts/build-sidecar.sh` → binary appears at `frontend/src-tauri/binaries/imageviz-backend-{triple}`.

### Task 3.2: Add Graceful Shutdown to Backend

**Files**: `backend/src/main.rs` (minimal changes)

**What**: The backend already has `shutdown_signal()` listening for SIGINT/SIGTERM (lines 242–260 of current `main.rs`). Tauri sends SIGKILL to sidecars on app exit by default. We need to handle SIGTERM properly and add a health endpoint Tauri can poll to know the backend is ready.

**Changes**:

```rust
// backend/src/main.rs — addition after the existing shutdown_signal()

/// Health endpoint already exists at GET /api/v1/health
/// It returns {"status":"ok"} — Tauri will poll this before showing the UI.

// The existing shutdown_signal() handles SIGTERM correctly.
// No changes needed — the sidecar receives SIGTERM when Tauri exits.
```

**Verification**: Start the backend, send `kill -TERM <pid>`, verify clean shutdown with Tantivy commit.

### Task 3.3: Add `tauri-plugin-shell` for Sidecar Management

**Files**: `frontend/src-tauri/Cargo.toml`, `frontend/src-tauri/src/lib.rs`

**What**: Add the shell plugin to the Tauri Rust core and register it.

```toml
# Cargo.toml — add dependency
tauri-plugin-shell = "2"
```

```rust
// src/lib.rs — register plugin
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            spawn_backend_sidecar(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn spawn_backend_sidecar(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri_plugin_shell::ShellExt;

    let sidecar = app.shell()
        .sidecar("imageviz-backend")
        .expect("sidecar not found in externalBin");

    let (mut rx, child) = sidecar
        .args(["--port", "3001"])
        .spawn()
        .expect("failed to spawn backend sidecar");

    // Store child for cleanup
    app.manage(std::sync::Mutex::new(child));

    let app_handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        use tauri_plugin_shell::process::CommandEvent;
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(line) => {
                    tracing::info!(target: "backend", "{}", String::from_utf8_lossy(&line));
                }
                CommandEvent::Stderr(line) => {
                    tracing::warn!(target: "backend", "{}", String::from_utf8_lossy(&line));
                }
                CommandEvent::Terminated(status) => {
                    tracing::info!(target: "backend", code = status.code, "Backend exited");
                }
                _ => {}
            }
        }
    });

    Ok(())
}
```

**Cleanup on exit**:

```rust
// In lib.rs — add cleanup handler
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            spawn_backend_sidecar(app)?;

            // Kill sidecar when all windows close
            let handle = app.handle().clone();
            app.on_window_event(move |_window, event| {
                if let tauri::WindowEvent::Destroyed = event {
                    if handle.windows().is_empty() {
                        // Cleanup handled by Tauri: sidecar receives SIGTERM
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error");
}
```

### Task 3.4: Add Sidecar Permissions (Capabilities)

**Files**: `frontend/src-tauri/capabilities/default.json` (amend)

**What**: Grant the shell plugin permission to spawn the sidecar. Without this, the sidecar won't start.

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default capability for the main window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    {
      "identifier": "shell:allow-execute",
      "allow": [
        {
          "name": "imageviz-backend",
          "sidecar": true,
          "args": true
        }
      ]
    },
    "shell:allow-spawn",
    "shell:allow-stdin-write"
  ]
}
```

### Task 3.5: Add Health Check and Ready Polling

**Files**: `frontend/src-tauri/src/lib.rs` (amend)

**What**: Wait for the backend to be ready before showing the UI. Poll `http://localhost:3001/api/v1/health` until it returns `200`.

```rust
// In lib.rs — after spawning sidecar
async fn wait_for_backend_ready() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let health_url = "http://localhost:3001/api/v1/health";

    for i in 0..30 {
        // 30 attempts × 500ms = 15s timeout
        if client.get(health_url).send().await.map(|r| r.status().is_success()).unwrap_or(false) {
            tracing::info!("Backend ready after {} attempts", i + 1);
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    Err("Backend failed to become ready within 15s".into())
}
```

### Task 3.6: Write Sidecar Tests

**Files**: `frontend/src-tauri/src/lib_test.rs` (NEW), `backend/tests/sidecar_shutdown_test.rs` (NEW)

**What**: Two test files:

1. **Rust test** (Tauri side): Mock the shell plugin, test that `spawn_backend_sidecar` calls `.sidecar("imageviz-backend")` correctly. Test that the health check loop retries.

2. **Backend integration test**: Send SIGTERM, verify Tantivy commits before exit. Verify that health endpoint returns 200 when ready.

```rust
// backend/tests/sidecar_shutdown_test.rs
#[tokio::test]
async fn health_endpoint_returns_ok_when_ready() {
    let app = create_test_app().await;
    let response = app.get("/api/v1/health").await;
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await;
    assert_eq!(body["status"], "ok");
}
```

### Task 3.7: End-to-End Dev Verification

**What**: Start the backend manually (`cargo run`), then `npm run tauri dev`. Verify:
- Tauri window opens
- Backend health check passes
- React app loads and fetches data normally
- Closing the Tauri window kills the backend (or doesn't — in dev mode the backend is separate)

### Phase 3 Exit Criteria

- [ ] `scripts/build-sidecar.sh` produces binary at the correct path with correct name
- [ ] `npm run tauri dev` spawns the backend (in production mode) or connects to existing (dev mode)
- [ ] Backend health endpoint responds within 15s of Tauri launch
- [ ] Closing Tauri window cleanly terminates the sidecar
- [ ] All existing backend tests pass (`cargo test`)
- [ ] All existing frontend tests pass (`npm test`)
- [ ] Sidecar shutdown test validates Tantivy commit on exit

---

## Phase 4: Frontend Adaptation (1–2 days)

**Goal**: Make the React frontend Tauri-aware without breaking the web build. This means:
- Conditional imports of Tauri-specific code
- Runtime detection of Tauri vs browser
- A clean abstraction layer so components don't know about the runtime

**Key principle**: No existing component should change. All Tauri adaptation happens in new files and in wrapper layers.

### Task 4.1: Create Runtime Detection Utility

**Files**: `frontend/src/utils/platform.ts` (NEW)

**What**: A single source of truth for runtime detection. Used by all other Tauri-aware code.

```typescript
/**
 * Runtime platform detection.
 *
 * Uses both compile-time and runtime checks:
 * - __TAURI_BUILD__: Set by Vite at build time (true in Tauri builds)
 * - core.isTauri: Runtime check (works in dev with Vite HMR)
 */

declare const __TAURI_BUILD__: boolean;

let _isTauriPromise: Promise<boolean> | null = null;

export function isTauriBuild(): boolean {
  return __TAURI_BUILD__;
}

export async function isRunningInTauri(): Promise<boolean> {
  if (_isTauriPromise) return _isTauriPromise;

  _isTauriPromise = (async () => {
    // Compile-time check: fast path for web builds
    if (!__TAURI_BUILD__) return false;

    // Runtime check: needed for dev mode where Tauri APIs are available
    try {
      const { core } = await import('@tauri-apps/api');
      return core.isTauri;
    } catch {
      return false;
    }
  })();

  return _isTauriPromise;
}
```

**Why async**: Dynamic imports prevent `@tauri-apps/api` from being loaded in web builds. The function is memoized so it only imports once.

### Task 4.2: Create API Client Bridge

**Files**: `frontend/src/api/tauri-client.ts` (NEW)

**What**: A thin wrapper that resolves the correct base URL for API calls depending on the runtime. In browser: `/api/v1` (Vite proxy). In Tauri: `http://localhost:3001/api/v1` (direct to sidecar).

```typescript
import { isRunningInTauri } from '@/utils/platform';

let _baseUrl: string | null = null;

/**
 * Resolve the API base URL for the current runtime.
 *
 * - Browser (dev): /api/v1 (Vite proxy → :3001)
 * - Browser (prod): /api/v1 (served by same origin or reverse proxy)
 * - Tauri: http://localhost:3001/api/v1 (direct to sidecar)
 */
export async function getApiBaseUrl(): Promise<string> {
  if (_baseUrl) return _baseUrl;

  if (await isRunningInTauri()) {
    _baseUrl = 'http://localhost:3001/api/v1';
  } else {
    _baseUrl = '/api/v1';
  }

  return _baseUrl;
}

/**
 * Perform a typed GET request, automatically resolving the correct base URL.
 */
export async function tauriGet<T>(
  path: string,
  params?: Record<string, string | number | undefined>,
): Promise<T> {
  const base = await getApiBaseUrl();
  const url = new URL(`${base}${path}`, window.location.origin);

  if (params) {
    for (const [key, value] of Object.entries(params)) {
      if (value !== undefined && value !== null) {
        url.searchParams.set(key, String(value));
      }
    }
  }

  const response = await fetch(url.toString());

  if (!response.ok) {
    throw new Error(`API error: ${response.status} ${response.statusText}`);
  }

  return response.json();
}
```

**Design decision**: We create `tauri-client.ts` as a new file rather than modifying `client.ts`. The existing `client.ts` continues to work for browser mode. In a future refactor, `client.ts` could delegate to this bridge, but that's out of scope for v1 — we're adding, not changing.

**Where the bridge is used**: Only in new Tauri-specific features (file dialogs, shortcuts) that need to call the API. The main data fetching (thumbnails, search, media) continues to use the existing `client.ts` which works fine in Tauri's WebView via the Vite proxy.

### Task 4.3: Create Tauri Feature Setup Hook

**Files**: `frontend/src/hooks/use-tauri-features.ts` (NEW)

**What**: A React hook that initializes Tauri-specific features when running in the desktop shell. Called once from `App.tsx` (conditionally).

```typescript
import { useEffect } from 'react';
import { isRunningInTauri } from '@/utils/platform';

/**
 * Initialize Tauri desktop features.
 * No-op in browser — safe to call unconditionally.
 */
export function useTauriFeatures() {
  useEffect(() => {
    let cleanup: (() => void) | undefined;

    isRunningInTauri().then((inTauri) => {
      if (!inTauri) return;

      // Phase 5 will add shortcut registration etc. here
      Promise.all([
        import('@/platform/desktop/shortcuts').then((m) => m.registerAppShortcuts()),
        import('@/platform/desktop/drag').then((m) => m.setupNativeDrag()),
      ]).then(([unregShortcuts, unregDrag]) => {
        cleanup = () => {
          unregShortcuts?.();
          unregDrag?.();
        };
      });
    });

    return () => {
      cleanup?.();
    };
  }, []);
}
```

**Integration in `App.tsx`** — minimal change, added at the top of the `App` component:

```typescript
// In App() function — ONE new line added:
function App() {
  useTauriFeatures(); // ← New: no-op in browser, activates Tauri features in desktop
  // ... rest of existing code unchanged ...
}
```

### Task 4.4: Add Tauri Mock to Test Setup

**Files**: `frontend/src/setup-tests.ts` (amend), `frontend/src/test-utils/mock-tauri.ts` (NEW)

**What**: Ensure all Tauri APIs are mocked in jsdom so existing tests don't break.

```typescript
// frontend/src/setup-tests.ts — additions

// Mock @tauri-apps/api for jsdom
vi.mock('@tauri-apps/api', () => ({
  core: { isTauri: false },
}));

vi.mock('@tauri-apps/plugin-shell', () => ({
  Command: { create: vi.fn(), sidecar: vi.fn() },
  open: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn().mockResolvedValue(null),
  save: vi.fn().mockResolvedValue(null),
  message: vi.fn(),
  ask: vi.fn().mockResolvedValue(true),
}));

vi.mock('@tauri-apps/plugin-global-shortcut', () => ({
  register: vi.fn().mockResolvedValue(undefined),
  unregister: vi.fn().mockResolvedValue(undefined),
  isRegistered: vi.fn().mockResolvedValue(false),
}));

// Ensure window.__TAURI__ is undefined in web mode
Object.defineProperty(window, '__TAURI__', {
  value: undefined,
  writable: true,
});
```

### Task 4.5: Write Platform Detection Tests

**Files**: `frontend/src/utils/__tests__/platform.test.ts` (NEW)

**What**: Test the platform detection utility in both modes.

```typescript
import { describe, it, expect } from 'vitest';
import { isTauriBuild } from '@/utils/platform';

describe('isTauriBuild', () => {
  it('returns false in web build (default test environment)', () => {
    // Vitest runs in jsdom → __TAURI_BUILD__ is false
    expect(isTauriBuild()).toBe(false);
  });
});

describe('isRunningInTauri', () => {
  it('returns false when Tauri API is not available', async () => {
    const { isRunningInTauri } = await import('@/utils/platform');
    const result = await isRunningInTauri();
    expect(result).toBe(false);
  });
});
```

### Task 4.6: Write API Client Bridge Tests

**Files**: `frontend/src/api/__tests__/tauri-client.test.ts` (NEW)

**What**: Test that the bridge resolves correct URLs.

```typescript
import { describe, it, expect } from 'vitest';
import { getApiBaseUrl } from '@/api/tauri-client';

describe('getApiBaseUrl', () => {
  it('returns /api/v1 in web mode', async () => {
    const url = await getApiBaseUrl();
    expect(url).toBe('/api/v1');
  });
});
```

### Task 4.7: Verify No Regression

**What**: Run the full existing test suite. All tests must pass.

```bash
npm test              # all Vitest tests
npx vitest run        # explicit run
npx tsc --noEmit      # type check
```

### Phase 4 Exit Criteria

- [ ] `isTauriBuild()` returns `false` in web mode (tested)
- [ ] `isRunningInTauri()` returns `false` in web mode (tested)
- [ ] `getApiBaseUrl()` returns `/api/v1` in web mode
- [ ] `useTauriFeatures()` is a no-op in browser (no errors, no side effects)
- [ ] `App.tsx` change is exactly one line: `useTauriFeatures();`
- [ ] All existing tests pass (200+ tests)
- [ ] TypeScript strict mode passes (`npm run typecheck`)
- [ ] `npm run tauri dev` still works (if Phase 2-3 are complete)
- [ ] No `@tauri-apps/*` imports in the web build bundle (verify with `npm run build && ls -la dist/assets/`)

---

## Phase 5: Desktop Features (1–2 days)

**Goal**: Add native desktop capabilities that make the Tauri version feel like a real desktop app: global keyboard shortcuts, native file/folder dialogs, and OS-level drag-and-drop enhancements.

### Task 5.1: Add Global Shortcuts

**Files**: 
- `frontend/src/platform/desktop/shortcuts.ts` (NEW)
- `frontend/src-tauri/src/lib.rs` (amend — register plugin)
- `frontend/src-tauri/capabilities/default.json` (amend — permissions)

**What**: Register global keyboard shortcuts that work even when the app window is not focused. Tauri's `global-shortcut` plugin fires events that the frontend can listen to.

**Install**:
```bash
cd frontend
npm install @tauri-apps/plugin-global-shortcut
# Rust: cargo add tauri-plugin-global-shortcut (in src-tauri/)
```

```toml
# frontend/src-tauri/Cargo.toml — add
tauri-plugin-global-shortcut = "2"
```

```rust
// frontend/src-tauri/src/lib.rs — register
.plugin(tauri_plugin_global_shortcut::Builder::new().build())
```

**Frontend shortcuts module**:

```typescript
// frontend/src/platform/desktop/shortcuts.ts

import { register, unregister } from '@tauri-apps/plugin-global-shortcut';
import { getCurrentWindow } from '@tauri-apps/api/window';

interface ShortcutBinding {
  keys: string;
  action: () => void;
  description: string;
}

const SHORTCUTS: ShortcutBinding[] = [
  {
    keys: 'CmdOrCtrl+F',
    action: () => {
      // Focus search bar — emit event to React
      document.querySelector<HTMLInputElement>('input[aria-label="Search media"]')?.focus();
    },
    description: 'Focus search',
  },
  {
    keys: 'CmdOrCtrl+,',
    action: () => {
      // Toggle config panel — dispatch custom event
      window.dispatchEvent(new CustomEvent('imageviz:toggle-config'));
    },
    description: 'Open settings',
  },
  {
    keys: 'Escape',
    action: () => {
      // Close detail view
      window.dispatchEvent(new CustomEvent('imageviz:close-detail'));
    },
    description: 'Close preview',
  },
  {
    keys: 'CmdOrCtrl+Shift+I',
    action: async () => {
      const win = getCurrentWindow();
      await win.toggleMaximize();
    },
    description: 'Toggle fullscreen',
  },
];

export async function registerAppShortcuts(): Promise<() => void> {
  const registered: string[] = [];

  for (const binding of SHORTCUTS) {
    try {
      await register(binding.keys, (event) => {
        if (event.state === 'Pressed') {
          binding.action();
        }
      });
      registered.push(binding.keys);
    } catch (error) {
      console.warn(`Failed to register shortcut "${binding.keys}":`, error);
    }
  }

  // Return cleanup function
  return async () => {
    for (const keys of registered) {
      try {
        await unregister(keys);
      } catch {
        // Best-effort cleanup
      }
    }
  };
}
```

**Permissions** (`capabilities/default.json` — add):

```json
{
  "permissions": [
    "global-shortcut:allow-register",
    "global-shortcut:allow-unregister",
    "global-shortcut:allow-is-registered"
  ]
}
```

### Task 5.2: Add Native Folder Picker Dialog

**Files**: 
- `frontend/src/platform/desktop/dialogs.ts` (NEW)
- `frontend/src-tauri/capabilities/default.json` (amend)

**What**: Tauri's native file dialog can select folders, which is a better UX than the browser's file input for configuring watched folders.

**Install**:
```bash
npm install @tauri-apps/plugin-dialog
cargo add tauri-plugin-dialog  # in src-tauri/
```

```typescript
// frontend/src/platform/desktop/dialogs.ts

import { open } from '@tauri-apps/plugin-dialog';
import { isRunningInTauri } from '@/utils/platform';

/**
 * Pick a folder using the native OS dialog (Tauri) or browser fallback.
 */
export async function pickFolder(): Promise<string | null> {
  if (await isRunningInTauri()) {
    const selected = await open({
      directory: true,
      multiple: false,
      title: 'Select a folder to watch',
    });
    return selected ?? null;
  }

  // Browser fallback: the existing config panel uses a different mechanism
  // This function is only called when Tauri features are available
  return null;
}
```

**Permissions** (`capabilities/default.json` — add):

```json
{
  "permissions": [
    "dialog:allow-open",
    "dialog:allow-save",
    "dialog:allow-message",
    "dialog:allow-ask"
  ]
}
```

### Task 5.3: Add Native OS Drag Enhancement

**Files**: 
- `frontend/src/platform/desktop/drag.ts` (NEW)
- `frontend/src-tauri/src/lib.rs` (amend — onDragDropEvent)

**What**: The existing `react-dnd` HTML5 backend already works in Tauri's WebView for drag-from-app-to-OS. Tauri's `onDragDropEvent` can enhance this by accepting OS file drops _into_ the app window (e.g., drop a folder to add as watched).

**Decision**: Keep `react-dnd` for drag-out (it works). Add Tauri's native drag-in only as a window-level event.

```typescript
// frontend/src/platform/desktop/drag.ts

import { getCurrentWindow } from '@tauri-apps/api/window';

export async function setupNativeDrag(): Promise<() => void> {
  const appWindow = getCurrentWindow();

  const unlisten = await appWindow.onDragDropEvent((event) => {
    if (event.payload.type === 'drop') {
      const paths = event.payload.paths;
      // Dispatch custom event that the config panel can listen to
      window.dispatchEvent(
        new CustomEvent('imageviz:files-dropped', {
          detail: { paths },
        }),
      );
    }
  });

  return unlisten;
}
```

### Task 5.4: Add Auto-Updater Foundation

**Files**: 
- `frontend/src/platform/desktop/updater.ts` (NEW)
- `frontend/src-tauri/tauri.conf.json` (amend — plugins.updater)

**What**: Tauri's auto-updater checks for new versions and downloads delta updates (1–5 MB). Configure the plugin but defer the release server setup to a later phase.

**Install**:
```bash
npm install @tauri-apps/plugin-updater @tauri-apps/plugin-process
cargo add tauri-plugin-updater  # in src-tauri/
```

```typescript
// frontend/src/platform/desktop/updater.ts

import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { isRunningInTauri } from '@/utils/platform';

/**
 * Check for app updates. Called on startup and periodically.
 */
export async function checkForUpdates(): Promise<void> {
  if (!(await isRunningInTauri())) return;

  try {
    const update = await check();
    if (!update) return;

    console.log(`Update available: ${update.version}`);
    console.log(`Release notes: ${update.body}`);

    // Download and install with progress logging
    await update.downloadAndInstall((event) => {
      switch (event.event) {
        case 'Started':
          console.log(`Downloading ${event.data.contentLength} bytes`);
          break;
        case 'Finished':
          console.log('Update downloaded');
          break;
      }
    });

    console.log('Update installed — relaunching');
    await relaunch();
  } catch (error) {
    console.warn('Update check failed:', error);
  }
}
```

**`tauri.conf.json` addition**:

```json
{
  "plugins": {
    "updater": {
      "pubkey": "PLACEHOLDER_PUBLIC_KEY",
      "endpoints": [
        "https://releases.imageviz.app/{{target}}/{{arch}}/{{current_version}}"
      ]
    }
  }
}
```

> **Note**: The updater requires a signing key pair and a release server. The plugin is installed and wired, but the release endpoint is a placeholder. Actual auto-update infrastructure is deferred to post-v1.

### Task 5.5: Write Desktop Feature Tests

**Files**: 
- `frontend/src/platform/desktop/__tests__/shortcuts.test.ts` (NEW)
- `frontend/src/platform/desktop/__tests__/dialogs.test.ts` (NEW)
- `frontend/src/platform/desktop/__tests__/drag.test.ts` (NEW)

**What**: Test that all Tauri-specific features are properly mocked in web mode and functional in Tauri mode.

```typescript
// shortcuts.test.ts
import { describe, it, expect, vi } from 'vitest';
import { register, unregister } from '@tauri-apps/plugin-global-shortcut';

// @tauri-apps/plugin-global-shortcut is mocked in setup-tests.ts

describe('global shortcuts', () => {
  it('register returns a cleanup function', async () => {
    // register is mocked to return undefined by default
    const result = await register('CmdOrCtrl+F', vi.fn());
    expect(result).toBeUndefined();
  });

  it('does not throw when registering from web mode', async () => {
    // In web mode, the mock should return silently
    await expect(register('CmdOrCtrl+K', vi.fn())).resolves.not.toThrow();
  });
});
```

### Phase 5 Exit Criteria

- [ ] `CmdOrCtrl+F` focuses the search bar (in Tauri)
- [ ] `Escape` closes the detail view (in Tauri)
- [ ] Native folder picker opens OS dialog (in Tauri)
- [ ] Dropping a folder onto the Tauri window triggers the config panel
- [ ] Auto-updater check runs on startup without crashing
- [ ] All desktop feature tests pass in web mode (mocked)
- [ ] All existing tests pass (`npm test`)
- [ ] `npm run dev` (browser) shows no Tauri features — no errors from missing APIs
- [ ] `npm run tauri dev` shows Tauri features working

---

## Phase 6: Build & Release Pipeline (1 day)

**Goal**: Configure CI to build the Tauri desktop app for Linux, macOS, and Windows. The web CI remains unchanged.

### Task 6.1: Update `package.json` for Tauri Builds

**Files**: `frontend/package.json` (amend)

**What**: Add a `build:tauri` script that builds for desktop.

```json
{
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "build:tauri": "npm run build && npm run tauri build",
    "tauri": "tauri"
  }
}
```

### Task 6.2: Create GitHub Actions Workflow for Tauri Build

**Files**: `.github/workflows/tauri-build.yml` (NEW)

**What**: A new CI workflow that runs in parallel with the existing `ci.yml`. It builds the Tauri app for Linux and optionally macOS/Windows.

```yaml
name: Tauri Build

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

permissions:
  contents: read

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  # ── Build Backend Sidecar ──────────────────────────────────────────
  build-sidecar:
    runs-on: ubuntu-latest
    timeout-minutes: 20
    defaults:
      run:
        working-directory: backend
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: backend
      - run: cargo build --release
      - name: Prepare sidecar binary
        run: |
          mkdir -p ../frontend/src-tauri/binaries
          cp target/release/imageviz-backend \
             ../frontend/src-tauri/binaries/imageviz-backend-x86_64-unknown-linux-gnu
      - uses: actions/upload-artifact@v4
        with:
          name: sidecar-linux-x86_64
          path: frontend/src-tauri/binaries/imageviz-backend-x86_64-unknown-linux-gnu
          if-no-files-found: error

  # ── Build Tauri Desktop App (Linux) ──────────────────────────────
  tauri-build-linux:
    needs: build-sidecar
    runs-on: ubuntu-latest
    timeout-minutes: 30
    defaults:
      run:
        working-directory: frontend
    steps:
      - uses: actions/checkout@v4

      - name: Install Tauri system dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libwebkit2gtk-4.1-dev \
            libayatana-appindicator3-dev \
            librsvg2-dev \
            libgtk-3-dev \
            libjavascriptcoregtk-4.1-dev \
            libsoup-3.0-dev

      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: frontend/src-tauri

      - uses: actions/setup-node@v4
        with:
          node-version: '22'
          cache: 'npm'
          cache-dependency-path: frontend/package-lock.json

      - run: npm ci

      - name: Download sidecar binary
        uses: actions/download-artifact@v4
        with:
          name: sidecar-linux-x86_64
          path: src-tauri/binaries/

      - name: Build Tauri app
        run: npm run tauri build -- --bundles deb,appimage

      - uses: actions/upload-artifact@v4
        with:
          name: tauri-linux-packages
          path: |
            frontend/src-tauri/target/release/bundle/deb/*.deb
            frontend/src-tauri/target/release/bundle/appimage/*.AppImage
          if-no-files-found: error

  # ── Sanity: Verify Web Build Still Works ──────────────────────────
  web-build-still-works:
    runs-on: ubuntu-latest
    timeout-minutes: 15
    defaults:
      run:
        working-directory: frontend
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '22'
          cache: 'npm'
          cache-dependency-path: frontend/package-lock.json
      - run: npm ci
      - run: npm run build
      - run: npm test
      - run: npm run typecheck
```

**Design notes**:

1. **`build-sidecar` job**: Compiles the Axum backend and provides it as an artifact. The `tauri-build-linux` job downloads it before bundling.
2. **Separate from existing CI**: The new workflow does not modify `ci.yml`. Both run in parallel on PRs.
3. **`web-build-still-works` job**: Guard that ensures Tauri changes haven't broken the web build.
4. **macOS/Windows**: Added as `tauri-build-macos` and `tauri-build-windows` jobs when those platforms are needed. For v1, Linux is sufficient.

### Task 6.3: Add Build Documentation

**Files**: `documents/plans/tauri-desktop-migration-plan.md` (this file)

**What**: Document the build commands developers need:

```bash
# Development (browser — unchanged)
cd frontend && npm run dev

# Development (Tauri desktop)
# Terminal 1: cd backend && cargo run
# Terminal 2: cd frontend && npm run tauri dev

# Production build (web — unchanged)
cd frontend && npm run build

# Production build (Linux desktop)
bash scripts/build-sidecar.sh
cd frontend && npm run build:tauri
# Output: frontend/src-tauri/target/release/bundle/

# Production build (all platforms — CI)
# Handled by .github/workflows/tauri-build.yml
```

### Task 6.4: Write Build Pipeline Tests

**Files**: (No new test files — this phase is validated by CI passing)

**What**: The CI workflow itself is the test. Verify:
- `build-sidecar` produces a valid binary
- `tauri-build-linux` bundles it correctly
- `web-build-still-works` confirms no regression

### Phase 6 Exit Criteria

- [ ] `npm run build` produces `frontend/dist/` as before (web build works)
- [ ] `bash scripts/build-sidecar.sh` produces `frontend/src-tauri/binaries/imageviz-backend-{triple}`
- [ ] `npm run build:tauri` produces `.deb` and `.AppImage` packages
- [ ] `.github/workflows/tauri-build.yml` passes on CI
- [ ] Existing `.github/workflows/ci.yml` passes on CI (unchanged)
- [ ] Build docs are complete

---

## Phase 7: E2E Testing & Polish (1 day)

**Goal**: Add end-to-end tests for the Tauri desktop app and validate performance targets.

### Task 7.1: Set Up WebDriver Testing

**Files**: 
- `frontend/e2e-tauri/` directory (NEW)
- `frontend/package.json` (amend — new devDeps)

**What**: Use Tauri's `tauri-driver` with WebDriverIO for full desktop app E2E tests.

**Install**:
```bash
npm install --save-dev @wdio/cli @wdio/local-runner @wdio/mocha-framework @wdio/spec-reporter
cargo install tauri-driver --locked
```

**CI prerequisite** (Linux): `sudo apt-get install -y webkit2gtk-driver xvfb`

**WebDriverIO config** (`frontend/e2e-tauri/wdio.conf.ts`):

```typescript
import { spawn, spawnSync } from 'child_process';
import path from 'path';
import os from 'os';

let tauriDriver: ReturnType<typeof spawn>;
let exit = false;

export const config: WebdriverIO.Config = {
  hostname: '127.0.0.1',
  port: 4444,
  specs: ['./specs/**/*.ts'],
  maxInstances: 1,
  capabilities: [
    {
      maxInstances: 1,
      'tauri:options': {
        application: path.resolve(__dirname, '..', 'src-tauri', 'target', 'debug', 'imageviz'),
      },
    },
  ],
  reporters: ['spec'],
  framework: 'mocha',
  mochaOpts: {
    ui: 'bdd',
    timeout: 60000,
  },

  onPrepare: () => {
    // Build the Tauri app in debug mode
    spawnSync('npm', ['run', 'tauri', 'build', '--', '--debug', '--no-bundle'], {
      cwd: path.resolve(__dirname, '..'),
      stdio: 'inherit',
      shell: true,
    });
  },

  beforeSession: () => {
    tauriDriver = spawn(
      path.resolve(os.homedir(), '.cargo', 'bin', 'tauri-driver'),
      [],
      { stdio: [null, process.stdout, process.stderr] },
    );

    tauriDriver.on('exit', (code) => {
      if (!exit) {
        console.error(`tauri-driver exited with code ${code}`);
        process.exit(1);
      }
    });
  },

  afterSession: () => {
    exit = true;
    tauriDriver?.kill();
  },
};
```

### Task 7.2: Write Tauri E2E Test Specs

**Files**: `frontend/e2e-tauri/specs/` (NEW)

**What**: Test critical desktop-specific user journeys.

```typescript
// e2e-tauri/specs/app-launch.spec.ts
describe('ImageViz Tauri App', () => {
  it('launches and shows the thumbnail grid', async () => {
    // Wait for the app to load
    const header = await $('header');
    await header.waitForExist({ timeout: 15000 });
    expect(await header.isDisplayed()).toBe(true);
  });

  it('renders thumbnail cards when media is available', async () => {
    // Wait for thumbnail grid to render
    const cards = await $$('[data-testid="thumbnail-card"]');
    // At least one card or empty state
    expect(cards.length).toBeGreaterThanOrEqual(0);
  });

  it('opens detail view on thumbnail click', async () => {
    const cards = await $$('[data-testid="thumbnail-card"]');
    if (cards.length > 0) {
      await cards[0].click();
      const detailView = await $('[data-testid="detail-view"]');
      await detailView.waitForExist({ timeout: 5000 });
      expect(await detailView.isDisplayed()).toBe(true);
    }
  });
});

// e2e-tauri/specs/global-shortcuts.spec.ts
describe('Global Shortcuts', () => {
  it('Ctrl+F focuses search bar', async () => {
    await browser.keys(['Control', 'f']);
    const searchInput = await $('input[aria-label="Search media"]');
    expect(await searchInput.isFocused()).toBe(true);
  });

  it('Escape closes detail view', async () => {
    // Open a detail view first
    const cards = await $$('[data-testid="thumbnail-card"]');
    if (cards.length > 0) {
      await cards[0].click();
      const detailView = await $('[data-testid="detail-view"]');
      await detailView.waitForExist({ timeout: 5000 });

      await browser.keys(['Escape']);
      // Detail view should close
      await detailView.waitForExist({ timeout: 3000, reverse: true });
    }
  });
});

// e2e-tauri/specs/backend-health.spec.ts
describe('Backend Sidecar', () => {
  it('health endpoint responds', async () => {
    // This test validates the sidecar is alive
    // We check indirectly via the app loading
    const body = await $('body');
    await body.waitForExist({ timeout: 20000 });
    expect(await body.isDisplayed()).toBe(true);
  });
});
```

### Task 7.3: Add Tauri E2E CI Job

**Files**: `.github/workflows/tauri-build.yml` (amend — add `tauri-e2e` job)

**What**: Run WebDriver tests in CI (Linux only for v1).

```yaml
  # ── Tauri E2E Tests (Linux) ─────────────────────────────────────
  tauri-e2e:
    needs: tauri-build-linux
    runs-on: ubuntu-latest
    timeout-minutes: 30
    defaults:
      run:
        working-directory: frontend
    steps:
      - uses: actions/checkout@v4

      - name: Install Tauri + WebDriver dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libwebkit2gtk-4.1-dev \
            libayatana-appindicator3-dev \
            webkit2gtk-driver \
            xvfb

      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - uses: actions/setup-node@v4
        with:
          node-version: '22'
          cache: 'npm'
          cache-dependency-path: frontend/package-lock.json

      - run: npm ci
      - run: cargo install tauri-driver --locked

      - name: Download sidecar binary
        uses: actions/download-artifact@v4
        with:
          name: sidecar-linux-x86_64
          path: src-tauri/binaries/

      - name: Run Tauri E2E tests
        run: xvfb-run npx wdio run e2e-tauri/wdio.conf.ts
```

### Task 7.4: Performance Validation

**Files**: No new files — manual measurement checklist.

**What**: After the Tauri build is working, validate against the performance targets from the platform options document:

| Metric | Target | Measurement Method | Status |
|--------|--------|-------------------|--------|
| Cold startup to interactive | < 500 ms | Wall clock: launch → grid visible | |
| Idle RAM | < 50 MB | OS process monitor (htop/task manager) | |
| Binary size | < 15 MB | `ls -lh` on the .deb/.AppImage | |
| Grid scroll FPS | 60 fps | Tauri DevTools → Performance tab | |
| Initial load (100 thumbnails) | < 1s | Wall clock: grid → all thumbnails loaded | |
| Search response (100K dataset) | < 200ms | Server-side timing (unchanged) | |
| SSE event latency | < 500ms | End-to-end: file added → UI update | |

### Task 7.5: Cross-Platform Smoke Test Checklist

**What**: Manual verification checklist for platforms beyond Linux.

**Linux** (primary):
- [ ] App opens from .deb installation
- [ ] App opens from .AppImage
- [ ] Thumbnails load and scroll smoothly
- [ ] Global shortcuts work (Ctrl+F, Escape)
- [ ] Video playback works
- [ ] Configuration panel works
- [ ] Drag from app to file manager works
- [ ] Closing the window cleanly exits

**macOS** (secondary):
- [ ] App bundle opens
- [ ] Menu bar shows app name
- [ ] Cmd+F, Cmd+, shortcuts work
- [ ] App appears in Dock
- [ ] DMG installer mounts correctly

**Windows** (secondary):
- [ ] NSIS installer runs
- [ ] App appears in Start menu
- [ ] Window decorations (min/max/close) work
- [ ] Uninstaller removes the app

### Phase 7 Exit Criteria

- [ ] Tauri WebDriver E2E tests pass on CI
- [ ] `app-launch.spec.ts` — app opens and renders within 15s
- [ ] `global-shortcuts.spec.ts` — Ctrl+F and Escape work
- [ ] `backend-health.spec.ts` — sidecar responds
- [ ] All existing tests pass (`npm test`, `cargo test`)
- [ ] Performance targets measured and documented
- [ ] Linux smoke test checklist complete
- [ ] macOS/Windows smoke test done (if hardware available) or deferred

---

## Dependency Graph (Internal to This Plan)

```
Phase 1 (Environment Isolation) ── independent, can start immediately
  │
  ├── Phase 2 (Tauri Shell Scaffold)
  │     │
  │     ├── Phase 3 (Sidecar Backend)
  │     │     │
  │     │     ├── Phase 4 (Frontend Adaptation)
  │     │     │     │
  │     │     │     ├── Phase 5 (Desktop Features)
  │     │     │     │     │
  │     │     │     │     ├── Phase 6 (Build & Release CI)
  │     │     │     │     │     │
  │     │     │     │     │     └── Phase 7 (E2E Testing & Polish)
  │     │     │     │     │
  │     │     │     │     Note: Phase 4 tasks (4.1, 4.2, 4.3, 4.4) can run in parallel
  │     │     │     │     Note: Phase 5 tasks (5.1, 5.2, 5.3, 5.4) can run in parallel
  │     │     │     │
  │     │     │     Note: Phase 3 and Phase 4 have partial overlap
  │     │     │           4.1, 4.4 (platform detection, test mocks) don't need Phase 3
  │     │     │           4.2, 4.3 (API bridge, hooks) also independent
  │     │     │
  │     │     Note: Phase 2 can be partially parallel with Phase 3 preparation
  │     │            Task 3.1 (build script) is independent
```

### Parallel Opportunities

| Batch | Phases | Rationale |
|-------|--------|-----------|
| **Batch 1** | P1 (all tasks) | Infrastructure — no dependencies |
| **Batch 2** | P2 (all tasks) | Tauri scaffold — depends on P1 |
| **Batch 3** | P3.1 (build script) + P4.1, P4.4 (platform, mocks) | Independent tasks |
| **Batch 4** | P3.2–3.7 (sidecar) + P4.2, P4.3, P4.5, P4.6 (bridge, hooks, tests) | P4 tasks mostly independent of sidecar |
| **Batch 5** | P5.1, P5.2, P5.3, P5.4 (desktop features) | All independent of each other |
| **Batch 6** | P6 (build pipeline) | Depends on P3+P4+P5 |
| **Batch 7** | P7 (E2E testing) | Depends on P6 (need builds) |

---

## Files Changed / Created Summary

### New Files (created by this plan)

```
frontend/
├── src/
│   ├── utils/
│   │   ├── platform.ts                     # P4.1: Runtime detection
│   │   └── __tests__/
│   │       └── platform.test.ts            # P4.5: Platform detection tests
│   ├── api/
│   │   ├── tauri-client.ts                 # P4.2: API bridge for Tauri
│   │   └── __tests__/
│   │       └── tauri-client.test.ts        # P4.6: API bridge tests
│   ├── hooks/
│   │   └── use-tauri-features.ts           # P4.3: Tauri feature init hook
│   ├── platform/
│   │   └── desktop/
│   │       ├── shortcuts.ts                # P5.1: Global shortcuts
│   │       ├── dialogs.ts                  # P5.2: Native dialogs
│   │       ├── drag.ts                     # P5.3: Native drag drop
│   │       ├── updater.ts                  # P5.4: Auto-updater
│   │       └── __tests__/
│   │           ├── shortcuts.test.ts       # P5.5
│   │           ├── dialogs.test.ts         # P5.5
│   │           └── drag.test.ts            # P5.5
│   ├── test-utils/
│   │   └── mock-tauri.ts                   # P4.4: Tauri mock helpers
│   └── __tests__/
│       └── environment/
│           ├── tauri-flag.test.ts          # P1.4: Build flag tests
│           └── tauri-config.test.ts        # P2.5: Config tests
├── src-tauri/                              # P2.3: Tauri Rust crate
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json                     # P2.2: Tauri config
│   ├── capabilities/
│   │   └── default.json                    # P3.4: Permissions
│   ├── icons/
│   │   └── .gitkeep
│   ├── binaries/
│   │   └── .gitkeep
│   └── src/
│       ├── main.rs                         # P2.3: Desktop entry point
│       ├── lib.rs                          # P3.3: Sidecar setup + plugins
│       └── lib_test.rs                     # P3.6: Sidecar tests
├── e2e-tauri/                              # P7.1-7.2: WebDriver E2E
│   ├── wdio.conf.ts
│   └── specs/
│       ├── app-launch.spec.ts
│       ├── global-shortcuts.spec.ts
│       └── backend-health.spec.ts
```

### Modified Files (existing files touched)

```
backend/
├── src/main.rs                             # P3.2: None (already has shutdown_signal)
└── tests/
    └── sidecar_shutdown_test.rs            # P3.6: NEW test file in existing dir

frontend/
├── package.json                            # P1.1: tauri script + new deps
├── vite.config.ts                          # P1.2-1.3: VITE_BACKEND_URL + __TAURI_BUILD__
├── src/
│   ├── vite-env.d.ts                       # P1.3: __TAURI_BUILD__ type
│   ├── App.tsx                             # P4.3: +1 line (useTauriFeatures)
│   └── setup-tests.ts                      # P4.4: Tauri API mocks

.github/workflows/
└── tauri-build.yml                         # P6.2: NEW workflow (existing ci.yml unchanged)

scripts/
└── build-sidecar.sh                        # P3.1: NEW build script

.gitignore                                  # P1.5: src-tauri/target/, src-tauri/gen/
```

### Unchanged Files (zero modifications)

All 60+ React components (`components/`), all hooks (except new `use-tauri-features`), all stores (Jotai atoms), all types, all existing tests, all backend source files (except main.rs change is zero in practice — it already has the needed shutdown handling).

---

## Risk Mitigation

| Risk | Phase | Mitigation |
|------|-------|------------|
| **WebView rendering differences** | P2 | CSS feature detection; test on all three platforms; Tauview (image viewer) proves this works |
| **Sidecar process coordination** | P3 | Health check loop before showing UI; cleanup on window close; 15s timeout before error |
| **Web build regresses** | All | `web-build-still-works` CI job; every commit passes `npm test` + `npm run build` |
| **Tauri API imports break web build** | P4 | Dynamic `import()` for all Tauri-specific code; `__TAURI_BUILD__` flag for tree-shaking |
| **CSP blocks API calls** | P2 | CSP configured with `http://localhost:3001` in connect-src, img-src, media-src |
| **Rust compile times slow iteration** | P3 | Frontend HMR still works (Vite dev server separate from Tauri); Tauri rebuild only for Rust changes |
| **Vitest mocks incomplete** | P4 | Centralized mock in `setup-tests.ts` + `mock-tauri.ts` utility; test both `isTauri: true` and `false` |
| **Linux library dependencies** | P2, P6, P7 | Documented in CI config; `apt-get install` commands in workflow; verified on ubuntu-latest |

---

## Rollback Plan

If the Tauri integration causes unresolvable issues:

1. **Delete `frontend/src-tauri/`** — removes all Tauri Rust code
2. **Remove `tauri` script** from `package.json` — no more `npm run tauri`
3. **Remove `@tauri-apps/*`** from `package.json` dependencies
4. **Remove `useTauriFeatures()`** from `App.tsx` (1 line)
5. **Revert `vite.config.ts`** `define` and `server.proxy` changes (or leave them — they're backward-compatible)
6. **Delete `.github/workflows/tauri-build.yml`**
7. **Delete `frontend/src/platform/desktop/`**, `frontend/e2e-tauri/`, `frontend/src/api/tauri-client.ts`, `frontend/src/utils/platform.ts`

**Impact**: The web app returns to its exact pre-Tauri state. No data loss, no code change to any component.

---

## What We Gain (User-Facing)

| Current Experience | After Tauri Migration |
|-------------------|----------------------|
| Open browser → type URL → wait for page load | Double-click app icon → instant (< 500ms) |
| Browser chrome takes screen space | Full window dedicated to content |
| Ctrl+F searches page content | Global Cmd/Ctrl+F searches media (even when app not focused) |
| Browser sandbox restricts drag behavior | Native OS drag-and-drop |
| ~500 MB RAM (Chrome + app) | ~45 MB RAM (Tauri app + sidecar) |
| Browser constantly drains battery | Minimal battery impact (~0.4%/hr) |
| Can't register as default file handler | Can register as default image/video viewer |

---

## Appendix A: Key Paths Reference

| Path | Purpose |
|------|---------|
| `frontend/src-tauri/tauri.conf.json` | Tauri configuration (window, CSP, bundle, sidecar) |
| `frontend/src-tauri/src/lib.rs` | Tauri Rust entry point (plugins, sidecar spawn, lifecycle) |
| `frontend/src-tauri/capabilities/default.json` | Permission model (shell, dialog, shortcuts, fs) |
| `frontend/src-tauri/binaries/` | Sidecar binary location (compiled Axum backend) |
| `frontend/src/utils/platform.ts` | Runtime detection (Tauri vs Browser) |
| `frontend/src/platform/desktop/` | Tauri-specific feature modules |
| `frontend/e2e-tauri/` | WebDriver E2E tests for Tauri app |
| `scripts/build-sidecar.sh` | Compile backend → place in binaries/ |
| `.github/workflows/tauri-build.yml` | CI for Tauri desktop builds |

## Appendix B: Tauri Plugin Dependency Map

| Plugin | npm Package | Rust Crate | Phase | Purpose |
|--------|------------|------------|-------|---------|
| Shell | `@tauri-apps/plugin-shell` | `tauri-plugin-shell` | P3 | Sidecar spawn + management |
| Global Shortcut | `@tauri-apps/plugin-global-shortcut` | `tauri-plugin-global-shortcut` | P5 | Keyboard shortcuts |
| Dialog | `@tauri-apps/plugin-dialog` | `tauri-plugin-dialog` | P5 | Native file/folder picker |
| Updater | `@tauri-apps/plugin-updater` | `tauri-plugin-updater` | P5 | Auto-update check |
| Process | `@tauri-apps/plugin-process` | `tauri-plugin-process` | P5 | Relaunch after update |
| FS | `@tauri-apps/plugin-fs` | Not installed | — | Not needed in v1 (backend handles FS) |

## Appendix C: Environment Variables

| Variable | Set By | Purpose | Default |
|----------|--------|---------|---------|
| `VITE_BACKEND_URL` | Developer / CI | Override API proxy target | `http://localhost:3001` |
| `TAURI_ENV` | `npm run tauri dev` / CI | Signals Tauri build mode | (unset = web) |
| `IMAGEVIZ_DB_PATH` | Backend env | SQLite location | `{data_dir}/imageviz.db` |
| `IMAGEVIZ_CACHE_DIR` | Backend env | Thumbnail cache | `{data_dir}/thumbnails` |
| `IMAGEVIZ_TANTIVY_DIR` | Backend env | Tantivy index | `{data_dir}/tantivy` |
| `PORT` | Backend env | Sidecar port | `3001` |
