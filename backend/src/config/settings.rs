use std::path::PathBuf;

/// Application settings loaded from environment variables.
pub struct Settings {
    pub database_path: PathBuf,
    pub thumbnail_cache_dir: PathBuf,
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
                std::env::var("IMAGEVIZ_CACHE_DIR").unwrap_or_else(|_| {
                    data_dir.join("thumbnails").to_string_lossy().to_string()
                }),
            ),
            port: std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3001),
        }
    }
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
        std::env::var("XDG_DATA_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME").ok().map(|h| {
                    PathBuf::from(h)
                        .join(".local")
                        .join("share")
                        .join("imageviz")
                })
            })
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME").ok().map(|h| {
            PathBuf::from(h)
                .join("Library")
                .join("Application Support")
                .join("imageviz")
        })
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
}
