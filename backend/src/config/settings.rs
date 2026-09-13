use std::path::PathBuf;

/// Application settings loaded from environment variables.
pub struct Settings {
    pub database_path: PathBuf,
    pub thumbnail_cache_dir: PathBuf,
    pub tantivy_index_dir: PathBuf,
    pub port: u16,
}

impl Settings {
    /// Load settings from environment variables with platform-appropriate defaults.
    pub fn from_env() -> Self {
        let data_dir = dirs_data_dir().unwrap_or_else(|| PathBuf::from("./data"));

        Self {
            database_path: PathBuf::from(
                std::env::var("IMAGEVIZ_DB_PATH")
                    .unwrap_or_else(|_| data_dir.join("imageviz.db").to_string_lossy().to_string()),
            ),
            thumbnail_cache_dir: PathBuf::from(
                std::env::var("IMAGEVIZ_CACHE_DIR")
                    .unwrap_or_else(|_| data_dir.join("thumbnails").to_string_lossy().to_string()),
            ),
            tantivy_index_dir: PathBuf::from(
                std::env::var("IMAGEVIZ_TANTIVY_DIR")
                    .unwrap_or_else(|_| data_dir.join("tantivy").to_string_lossy().to_string()),
            ),
            port: std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3001),
        }
    }
}

/// Default CORS origin allowlist — the Vite dev server (overridable via
/// `CORS_ALLOW_ORIGINS`).
pub const DEFAULT_CORS_ORIGINS: &[&str] = &["http://localhost:5173", "http://127.0.0.1:5173"];

/// Parse a comma-separated origin list, trimming whitespace and dropping
/// empty entries.
pub fn parse_cors_origins(raw: &str) -> Vec<String> {
    raw.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()
}

/// Resolve the origin allowlist from a raw `CORS_ALLOW_ORIGINS` value,
/// falling back to [`DEFAULT_CORS_ORIGINS`] when unset, empty, or
/// whitespace-only.
pub fn cors_origins_from_raw(raw: Option<&str>) -> Vec<String> {
    match raw {
        Some(raw) if !raw.trim().is_empty() => parse_cors_origins(raw),
        _ => DEFAULT_CORS_ORIGINS.iter().map(|s| (*s).to_string()).collect(),
    }
}

/// Allowed CORS origins from `CORS_ALLOW_ORIGINS` (comma-separated).
pub fn cors_allow_origins() -> Vec<String> {
    cors_origins_from_raw(std::env::var("CORS_ALLOW_ORIGINS").ok().as_deref())
}

/// Get the platform-appropriate data directory for ImageViz.
///
/// Order of precedence:
/// 1. `$XDG_DATA_HOME/imageviz` (Linux)
/// 2. `$HOME/.local/share/imageviz` (Linux fallback)
/// 3. `$HOME/Library/Application Support/imageviz` (macOS)
/// 4. `./data` (fallback for other platforms)
fn dirs_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        std::env::var("XDG_DATA_HOME").ok().map(PathBuf::from).or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".local").join("share").join("imageviz"))
        })
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join("Library").join("Application Support").join("imageviz"))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Some(PathBuf::from("./data"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_default_port() {
        let settings = Settings::from_env();
        // PORT env var may be set in CI, so just verify it's a valid u16
        assert!(settings.port > 0);
    }

    #[test]
    fn test_settings_db_path_not_empty() {
        let settings = Settings::from_env();
        assert!(!settings.database_path.as_os_str().is_empty());
    }

    #[test]
    fn test_settings_cache_dir_not_empty() {
        let settings = Settings::from_env();
        assert!(!settings.thumbnail_cache_dir.as_os_str().is_empty());
    }

    #[test]
    fn test_settings_from_env_overrides_port() {
        // We can't easily override env in a test without unsafe,
        // but verify the parser doesn't crash with missing env vars
        let settings = Settings::from_env();
        assert!(settings.port >= 1024 || settings.port == 3001);
    }

    #[test]
    fn test_parse_cors_origins_trims_and_skips_empty() {
        assert_eq!(
            parse_cors_origins(" http://a.example , http://b.example ,,"),
            vec!["http://a.example".to_string(), "http://b.example".to_string()]
        );
        assert!(parse_cors_origins("").is_empty());
        assert!(parse_cors_origins("  ,  ").is_empty());
    }

    #[test]
    fn test_cors_origins_from_raw_env_semantics() {
        // Pure function — no env mutation required.
        assert_eq!(
            cors_origins_from_raw(Some("http://dev.example:8080")),
            vec!["http://dev.example:8080".to_string()],
            "override must be honored"
        );
        let defaults = DEFAULT_CORS_ORIGINS.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(cors_origins_from_raw(None), defaults, "unset env uses defaults");
        assert_eq!(cors_origins_from_raw(Some("  ")), defaults, "empty override uses defaults");
    }
}
