# Wave 6.4 — Implement Folder Path Suggestion (Server-Side)

| Field | Value |
|-------|-------|
| **Wave** | 6 — Frontend: Real-time SSE, Config UI & Polish |
| **Seq** | 04 |
| **Estimate** | 1 hour |
| **Depends on** | 1.2 (config management) |
| **Parallel** | Can run in parallel with 6.3 |

---

## Overview

Add a backend endpoint that suggests subdirectories for a given path prefix. This enables autocomplete-style folder suggestions in the configuration panel's path input, helping users discover and select folders without typing full paths.

## Prerequisites

- Config management (1.2)
- File system access from the backend

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 6 task 6.4
- `.opencode/context/development/principles/api-design.md`

## Deliverables

```
backend/src/routes/
└── config.rs                    # Updated: add GET /config/suggest endpoint
```

## Acceptance Criteria (Pass/Fail)

- [ ] `GET /api/v1/config/suggest?path=~/` returns list of subdirectories under `~/`
- [ ] Response format: `{ "suggestions": [{ "path": "/full/path", "name": "dirname", "is_directory": true }] }`
- [ ] Paths are absolute (resolved from `~` to `$HOME`)
- [ ] Only directories are returned (files excluded)
- [ ] Hidden directories (starting with `.`) are excluded
- [ ] Empty suggestion list returned for nonexistent paths (no error)
- [ ] Maximum 50 suggestions returned
- [ ] Response time < 100ms (simple directory listing)

## Implementation Notes

**Handler:**
```rust
use axum::{extract::Query, Json};
use std::path::Path;

#[derive(Deserialize)]
struct SuggestParams {
    path: String,
}

#[derive(Serialize)]
struct SuggestResponse {
    suggestions: Vec<PathSuggestion>,
}

#[derive(Serialize)]
struct PathSuggestion {
    path: String,
    name: String,
    is_directory: bool,
}

async fn suggest_folders(
    Query(params): Query<SuggestParams>,
) -> Result<Json<SuggestResponse>, AppError> {
    // Resolve ~ to $HOME
    let resolved = resolve_path(&params.path);
    let path = Path::new(&resolved);
    
    // Get parent directory and prefix for partial matching
    let (search_dir, prefix) = if path.exists() && path.is_dir() {
        (path.to_path_buf(), String::new())
    } else {
        // Path might be incomplete — search in parent dir
        let parent = path.parent().unwrap_or(Path::new("/"));
        let prefix = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        (parent.to_path_buf(), prefix)
    };
    
    let mut suggestions = Vec::new();
    
    if let Ok(entries) = std::fs::read_dir(&search_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            
            // Skip hidden
            if name.starts_with('.') { continue; }
            
            // Filter by prefix
            if !prefix.is_empty() && !name.starts_with(&prefix) { continue; }
            
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            if !is_dir { continue; } // Only directories
            
            suggestions.push(PathSuggestion {
                path: entry.path().to_string_lossy().to_string(),
                name,
                is_directory: is_dir,
            });
        }
    }
    
    // Sort alphabetically, limit to 50
    suggestions.sort_by(|a, b| a.name.cmp(&b.name));
    suggestions.truncate(50);
    
    Ok(Json(SuggestResponse { suggestions }))
}

fn resolve_path(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return path.replacen("~", &home, 1);
        }
    }
    path.to_string()
}
```

**Route mounting:**
```rust
// In config routes
pub fn routes() -> Router {
    Router::new()
        .route("/config", get(get_config).put(update_config))
        .route("/config/suggest", get(suggest_folders))
}
```

## Test Strategy

```rust
#[tokio::test]
async fn test_suggest_root_directories() {
    let app = test_app().await;
    
    let response = app.get("/api/v1/config/suggest?path=/")
        .send().await;
    
    assert_eq!(response.status(), 200);
    let body: SuggestResponse = response.json().await;
    assert!(!body.suggestions.is_empty());
    // Should include common dirs like /tmp, /home, /etc
    assert!(body.suggestions.iter().any(|s| s.name == "tmp"));
}

#[tokio::test]
async fn test_suggest_nonexistent_path_returns_empty() {
    let app = test_app().await;
    let response = app.get("/api/v1/config/suggest?path=/nonexistent/xyz")
        .send().await;
    
    assert_eq!(response.status(), 200);
    let body: SuggestResponse = response.json().await;
    assert!(body.suggestions.is_empty());
}

#[tokio::test]
async fn test_suggest_excludes_hidden() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".hidden_dir")).unwrap();
    std::fs::create_dir(dir.path().join("visible_dir")).unwrap();
    
    let app = test_app().await;
    let response = app.get(&format!("/api/v1/config/suggest?path={}", dir.path().display()))
        .send().await;
    
    let body: SuggestResponse = response.json().await;
    assert!(body.suggestions.iter().any(|s| s.name == "visible_dir"));
    assert!(!body.suggestions.iter().any(|s| s.name == ".hidden_dir"));
}
```
