//! Navidrome / OpenSubsonic configuration management.
//!
//! Stores server URL, username, and password in `~/.config/smyx/navidrome.toml`.

use std::fs;
use std::path::PathBuf;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NavidromeConfig {
    pub server_url: String,
    pub username: String,
    pub password: String,
}

impl NavidromeConfig {
    pub fn new(server_url: String, username: String, password: String) -> Self {
        Self {
            server_url: Self::normalize_url(&server_url),
            username: username.trim().to_string(),
            password,
        }
    }

    /// Ensure URL has a scheme (default to `http://`) and no trailing slash.
    pub fn normalize_url(url: &str) -> String {
        let trimmed = url.trim();
        let prefixed = if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
            format!("http://{}", trimmed)
        } else {
            trimmed.to_string()
        };
        prefixed.trim_end_matches('/').to_string()
    }

    pub fn config_path() -> Option<PathBuf> {
        crate::home_dir().map(|h| h.join(".config/smyx/navidrome.toml"))
    }

    pub fn load() -> Option<Self> {
        let path = Self::config_path()?;
        let content = fs::read_to_string(&path).ok()?;
        let mut cfg: NavidromeConfig = toml::from_str(&content).ok()?;
        cfg.server_url = Self::normalize_url(&cfg.server_url);
        if cfg.server_url.is_empty() || cfg.username.is_empty() {
            None
        } else {
            Some(cfg)
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path().context("unable to determine config path")?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("create config directory")?;
        }
        let content = toml::to_string_pretty(self).context("serialize navidrome config")?;
        fs::write(&path, content).context("write navidrome config to disk")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_url() {
        assert_eq!(
            NavidromeConfig::normalize_url("192.168.1.100:4533/"),
            "http://192.168.1.100:4533"
        );
        assert_eq!(
            NavidromeConfig::normalize_url("https://music.example.com/"),
            "https://music.example.com"
        );
        assert_eq!(
            NavidromeConfig::normalize_url("  http://localhost:4533  "),
            "http://localhost:4533"
        );
    }
}
