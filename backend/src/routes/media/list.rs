use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::middleware::validation;

use super::MediaState;

// ---------------------------------------------------------------------------
// Media list (cursor-based pagination)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(super) struct MediaListParams {
    #[serde(default = "default_limit")]
    limit: u32,
    cursor: Option<String>,
    cursor_id: Option<String>,
    mime_type: Option<String>,
}

fn default_limit() -> u32 {
    100
}

#[derive(Serialize)]
pub(super) struct MediaItemSummary {
    pub id: String,
    pub filename: String,
    pub path: String,
    pub mime_type: String,
    pub thumbnail_url: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub file_size: i64,
    pub created_at: String,
    pub modified_at: String,
}

/// GET /api/v1/media — list media items with cursor-based pagination.
///
/// Query parameters:
/// - `limit` (default 100, max 500): number of items per page
/// - `cursor` (ISO 8601 date): exclusive cursor from the last item's `created_at`
/// - `cursor_id` (UUID): tiebreaker for items with the same `file_created_at`
/// - `mime_type` (e.g. `image/%`): optional MIME type filter (SQL LIKE)
///
/// Returns a JSON object with `data` (array of `MediaItemSummary`) and `meta`
/// (pagination metadata: `next_cursor`, `next_cursor_id`, `has_more`, `total`).
pub(super) async fn list_media(
    State(state): State<Arc<MediaState>>,
    Query(params): Query<MediaListParams>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    validation::validate_limit(params.limit)?;
    validation::validate_cursor(params.cursor.as_deref())?;
    validation::validate_cursor_id(params.cursor_id.as_deref())?;

    let limit = params.limit;
    let fetch_limit = limit + 1;
    let has_cursor = params.cursor.is_some() && params.cursor_id.is_some();
    let has_mime = params.mime_type.is_some();

    let conn = state.db.get().map_err(|e| {
        tracing::error!(error = %e, "Failed to acquire database connection");
        (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Service temporarily unavailable"})))
    })?;

    // Compute total count (fast COUNT with or without mime_type filter)
    let total: i64 = if let Some(ref mime_type) = params.mime_type {
        conn.query_row(
            "SELECT COUNT(*) FROM media_items WHERE mime_type LIKE ?1",
            rusqlite::params![mime_type],
            |row| row.get(0),
        )
        .unwrap_or(0)
    } else {
        conn.query_row("SELECT COUNT(*) FROM media_items", [], |row| row.get(0)).unwrap_or(0)
    };

    // Build SQL dynamically for cursor-based pagination
    let mut sql = String::from(
        "SELECT id, filename, relative_path, mime_type, width, height, file_size, \
         file_created_at, file_modified_at FROM media_items",
    );

    let mut where_parts: Vec<String> = Vec::new();
    let mut next_param = 1;
    if has_cursor {
        where_parts.push(format!("(file_created_at, id) < (?{}, ?{})", next_param, next_param + 1));
        next_param += 2;
    }
    if has_mime {
        where_parts.push(format!("mime_type LIKE ?{}", next_param));
        next_param += 1;
    }

    if !where_parts.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&where_parts.join(" AND "));
    }

    sql.push_str(" ORDER BY file_created_at DESC, id DESC LIMIT ?");
    sql.push_str(&next_param.to_string());

    // Collect parameter values in the same order as their placeholders
    let mut values: Vec<rusqlite::types::Value> = Vec::new();
    if let (Some(cursor), Some(cursor_id)) = (&params.cursor, &params.cursor_id) {
        values.push(rusqlite::types::Value::Text(cursor.clone()));
        values.push(rusqlite::types::Value::Text(cursor_id.clone()));
    }
    if let Some(ref mime_type) = params.mime_type {
        values.push(rusqlite::types::Value::Text(mime_type.clone()));
    }
    values.push(rusqlite::types::Value::Integer(fetch_limit as i64));

    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        values.iter().map(|v| v as &dyn rusqlite::types::ToSql).collect();

    let mut items: Vec<MediaItemSummary> = {
        let mut stmt = conn.prepare(&sql).map_err(|e| {
            tracing::error!(error = %e, "Failed to prepare media list query");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Internal server error"})))
        })?;

        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                let id: String = row.get(0)?;
                let filename: String = row.get(1)?;
                let relative_path: String = row.get(2)?;
                let mime_type: String = row.get(3)?;
                let width: Option<i64> = row.get(4)?;
                let height: Option<i64> = row.get(5)?;
                let file_size: i64 = row.get(6)?;
                let file_created_at: String = row.get(7)?;
                let file_modified_at: String = row.get(8)?;
                Ok(MediaItemSummary {
                    thumbnail_url: format!("/api/v1/media/{}/thumbnail", id),
                    id,
                    filename,
                    path: relative_path,
                    mime_type,
                    width,
                    height,
                    file_size,
                    created_at: file_created_at,
                    modified_at: file_modified_at,
                })
            })
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to query media items");
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Internal server error"})))
            })?;

        let mut items: Vec<MediaItemSummary> = Vec::new();
        for row in rows {
            match row {
                Ok(item) => items.push(item),
                Err(e) => {
                    tracing::error!(error = %e, "Failed to read media row");
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": "Internal server error"})),
                    ));
                }
            }
        }
        items
    };

    drop(conn);

    let has_more = items.len() > limit as usize;
    items.truncate(limit as usize);

    let (next_cursor, next_cursor_id) = if has_more {
        let last = items.last().expect("items non-empty when has_more is true");
        (Some(last.created_at.clone()), Some(last.id.clone()))
    } else {
        (None, None)
    };

    Ok(Json(json!({
        "data": items,
        "meta": {
            "next_cursor": next_cursor,
            "next_cursor_id": next_cursor_id,
            "has_more": has_more,
            "total": total,
        }
    })))
}
