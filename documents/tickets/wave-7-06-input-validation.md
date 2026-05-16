# Wave 7.6 — Add Input Validation and Sanitization for All Endpoints

| Field | Value |
|-------|-------|
| **Wave** | 7 — Production Readiness & Hardening |
| **Seq** | 06 |
| **Estimate** | 1.5 hours |
| **Depends on** | All route files (Waves 1–3) |
| **Parallel** | No |

---

## Overview

Add robust input validation and sanitization to all API endpoints. Validate query parameters, path parameters, and request bodies. Return HTTP 400 with descriptive error messages for invalid inputs. This prevents crashes, SQL injection, and malformed data.

## Prerequisites

- All route handlers implemented (Waves 1–3)
- Backend running

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 7 task 7.6, §3 API Contract
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

Changes to all route files: `media.rs`, `search.rs`, `config.rs`, `stats.rs`

## Acceptance Criteria (Pass/Fail)

- [ ] All query parameters validated for type, range, and format:
  - `limit`: must be integer, 1–500
  - `cursor`: must be valid ISO 8601 date string
  - `cursor_id`: must be valid UUID
  - `q`: must be non-empty string (for search endpoint)
  - `width`: must be integer, 100–500 (for thumbnail endpoint)
- [ ] Path parameters validated:
  - `id`: must be non-empty string (UUID format recommended but not strictly enforced)
- [ ] Request bodies validated (for PUT /config):
  - `watched_folders` must be an array
  - Each folder must have a non-empty `path` string
  - `path` shouldn't contain injection characters (.. traversal)
- [ ] Invalid inputs return HTTP 400 with JSON:
  ```json
  { "error": "Invalid input", "details": [{ "field": "limit", "message": "Limit must be between 1 and 500" }] }
  ```
- [ ] Path traversal protection: config paths shouldn't allow `..` to escape watched folders
- [ ] All input length limits enforced (max query length: 1000 chars, max path length: 4096 chars)

## Implementation Notes

**Validation helper:**
```rust
use axum::http::StatusCode;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ValidationError {
    pub error: String,
    pub details: Vec<FieldError>,
}

#[derive(Debug, Serialize)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

impl ValidationError {
    pub fn new(message: &str) -> Self {
        Self {
            error: message.to_string(),
            details: vec![],
        }
    }

    pub fn with_details(message: &str, details: Vec<FieldError>) -> Self {
        Self {
            error: message.to_string(),
            details,
        }
    }

    pub fn into_response(self) -> (StatusCode, Json<Self>) {
        (StatusCode::BAD_REQUEST, Json(self))
    }
}
```

**Per-endpoint validation examples:**

Search endpoint:
```rust
fn validate_search_params(params: &SearchParams) -> Result<(), ValidationError> {
    let mut errors = Vec::new();
    
    if params.q.trim().is_empty() {
        errors.push(FieldError {
            field: "q".to_string(),
            message: "Search query cannot be empty".to_string(),
        });
    }
    
    if params.q.len() > 1000 {
        errors.push(FieldError {
            field: "q".to_string(),
            message: "Search query must be under 1000 characters".to_string(),
        });
    }
    
    if let Some(limit) = params.limit {
        if limit < 1 || limit > 500 {
            errors.push(FieldError {
                field: "limit".to_string(),
                message: "Limit must be between 1 and 500".to_string(),
            });
        }
    }
    
    if !errors.is_empty() {
        return Err(ValidationError::with_details("Invalid search parameters", errors));
    }
    
    Ok(())
}
```

Media list endpoint:
```rust
fn validate_media_list_params(params: &MediaListParams) -> Result<(), ValidationError> {
    let mut errors = Vec::new();
    
    if let Some(ref cursor) = params.cursor {
        if chrono::DateTime::parse_from_rfc3339(cursor).is_err() {
            errors.push(FieldError {
                field: "cursor".to_string(),
                message: "Cursor must be a valid ISO 8601 date".to_string(),
            });
        }
    }
    
    if let Some(ref cursor_id) = params.cursor_id {
        if uuid::Uuid::parse_str(cursor_id).is_err() {
            errors.push(FieldError {
                field: "cursor_id".to_string(),
                message: "cursor_id must be a valid UUID".to_string(),
            });
        }
    }
    
    // ...
    Ok(())
}
```

Config endpoint (PUT):
```rust
fn validate_config_body(body: &UpdateConfigRequest) -> Result<(), ValidationError> {
    let mut errors = Vec::new();
    
    for (i, folder) in body.watched_folders.iter().enumerate() {
        if folder.path.trim().is_empty() {
            errors.push(FieldError {
                field: format!("watched_folders[{}].path", i),
                message: "Path cannot be empty".to_string(),
            });
        }
        
        // Path traversal check
        if folder.path.contains("..") {
            errors.push(FieldError {
                field: format!("watched_folders[{}].path", i),
                message: "Path contains invalid characters".to_string(),
            });
        }
        
        if folder.path.len() > 4096 {
            errors.push(FieldError {
                field: format!("watched_folders[{}].path", i),
                message: "Path exceeds maximum length of 4096 characters".to_string(),
            });
        }
    }
    
    // ...
}
```

**Handler integration:**
```rust
async fn search(
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>, AppError> {
    validate_search_params(&params)?; // Returns AppError with 400 on failure
    // ... proceed with search
}
```

## Test Strategy

```rust
#[tokio::test]
async fn test_search_empty_query_returns_400() {
    let app = test_app().await;
    let response = app.get("/api/v1/search?q=").send().await;
    assert_eq!(response.status(), 400);
    
    let body: serde_json::Value = response.json().await;
    assert_eq!(body["error"], "Invalid search parameters");
}

#[tokio::test]
async fn test_limit_exceeds_max_returns_400() {
    let app = test_app().await;
    let response = app.get("/api/v1/media?limit=1000").send().await;
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn test_invalid_cursor_returns_400() {
    let app = test_app().await;
    let response = app.get("/api/v1/media?cursor=not-a-date").send().await;
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn test_path_traversal_in_config_returns_400() {
    let app = test_app().await;
    let response = app.put("/api/v1/config")
        .json(&json!({"watched_folders": [{"path": "../../etc"}]}))
        .send().await;
    assert_eq!(response.status(), 400);
}
```
