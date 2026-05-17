use axum::{extract::Query, http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

/// Query parameters for `GET /config/suggest`.
#[derive(Deserialize)]
pub(super) struct SuggestParams {
    path: String,
}

/// A single path suggestion returned by the suggest endpoint.
#[derive(Serialize)]
pub(super) struct PathSuggestion {
    path: String,
    name: String,
    is_directory: bool,
}

/// Response body for `GET /config/suggest`.
#[derive(Serialize)]
pub(super) struct SuggestResponse {
    suggestions: Vec<PathSuggestion>,
}

/// Expand a leading `~` to the user's home directory.
fn resolve_path(path: &str) -> String {
    if path == "~" {
        return std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        let mut resolved = home;
        resolved.push('/');
        resolved.push_str(rest);
        return resolved;
    }
    path.to_string()
}

/// GET /config/suggest — return subdirectory suggestions for a path prefix.
///
/// Used by the frontend config panel to power a folder autocomplete.  This
/// is a stateless filesystem operation — no database access is needed.
pub(super) async fn suggest_folders(
    Query(params): Query<SuggestParams>,
) -> Result<Json<SuggestResponse>, (StatusCode, Json<Value>)> {
    let resolved = resolve_path(&params.path);
    let path = Path::new(&resolved);

    let (search_dir, prefix) = if path.exists() && path.is_dir() {
        (path.to_path_buf(), String::new())
    } else {
        let parent = path.parent().unwrap_or(Path::new("/"));
        let prefix = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        (parent.to_path_buf(), prefix)
    };

    let mut suggestions = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&search_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // Skip hidden entries
            if name.starts_with('.') {
                continue;
            }
            // Filter by prefix when the path doesn't exist as a directory
            if !prefix.is_empty() && !name.starts_with(&prefix) {
                continue;
            }
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            if !is_dir {
                continue;
            }
            suggestions.push(PathSuggestion {
                path: entry.path().to_string_lossy().to_string(),
                name,
                is_directory: is_dir,
            });
        }
    }

    suggestions.sort_by(|a, b| a.name.cmp(&b.name));
    suggestions.truncate(50);

    Ok(Json(SuggestResponse { suggestions }))
}
