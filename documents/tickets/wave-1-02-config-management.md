# Wave 1.2 — Implement Configuration Management (Watched Folders)

| Field | Value |
|-------|-------|
| **Wave** | 1 — Backend: File System Scanner & Metadata Extraction |
| **Seq** | 02 |
| **Estimate** | 1.5 hours |
| **Depends on** | 1.1 (SQLite schema) |
| **Parallel** | No |

---

## Overview

Implement the configuration API endpoints and storage. Users can GET the current watched folder configuration and PUT updated watched folder paths. The config is persisted in the SQLite `config` table.

## Prerequisites

- SQLite schema and DB module (from 1.1)
- Axum routes module structure (from 0.4)

## Reference Files

- `documents/plans/development-plan.md` — §3.2 Endpoints (GET/PUT /config), §4 config table
- `.opencode/context/development/principles/api-design.md` — REST patterns

## Deliverables

```
backend/src/config/
├── mod.rs                       # Config struct, load/save functions
└── settings.rs                  # Env-based configuration (DB path, etc.)

backend/src/routes/
└── config.rs                    # GET /config, PUT /config handlers
```

## Acceptance Criteria (Pass/Fail)

- [ ] `GET /api/v1/config` returns JSON with `watched_folders` array
- [ ] `PUT /api/v1/config` with `{"watched_folders": ["/path/to/images"]}` persists to DB
- [ ] Subsequent `GET /api/v1/config` returns the updated folders
- [ ] Invalid PUT body returns HTTP 400 with descriptive error
- [ ] Configuration is loaded from DB on server startup
- [ ] `watched_folders` supports multiple paths (array of strings)
- [ ] Each watched folder entry has a `path` (required) and optional `label` (string)

## Implementation Notes

**Config struct:**
```rust
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WatchedFolder {
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub watched_folders: Vec<WatchedFolder>,
}
```

**Storage approach** — Since SQLite config table is key-value, serialize the watched folders as JSON:
- Key: `"watched_folders"`
- Value: JSON array of `WatchedFolder` objects

**Config module (`config/mod.rs`):**
```rust
pub fn load_config(conn: &Connection) -> Result<AppConfig, Error> { ... }
pub fn save_config(conn: &Connection, config: &AppConfig) -> Result<(), Error> { ... }
```

**Defaults** — Return empty `watched_folders: []` if no config exists in DB.

**PUT handler validation:**
- Validate paths are non-empty strings
- Validate paths exist (or warn if they don't — don't block saving)
- Return 400 for malformed JSON

## Test Strategy

- TDD: Write a failing test for `GET /config` returning empty config
- Write a failing test for `PUT /config` storing watched folders
- Write a failing test for invalid PUT body returning 400
- Integration test: Start with empty DB → PUT config → GET config → verify roundtrip

```rust
#[tokio::test]
async fn config_roundtrip() {
    let app = test_app().await; // helper that creates router with in-memory DB
    
    // Initially empty
    let resp = app.get("/api/v1/config").send().await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await;
    assert!(body["watched_folders"].as_array().unwrap().is_empty());
    
    // Update
    let resp = app.put("/api/v1/config")
        .json(&json!({"watched_folders": [{"path": "/tmp/test"}]}))
        .send().await;
    assert_eq!(resp.status(), 200);
    
    // Verify stored
    let resp = app.get("/api/v1/config").send().await;
    let body: serde_json::Value = resp.json().await;
    assert_eq!(body["watched_folders"][0]["path"], "/tmp/test");
}
```
