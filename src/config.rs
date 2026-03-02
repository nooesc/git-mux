use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(default = "default_general")]
    pub general: GeneralConfig,
    #[serde(default)]
    pub orgs: OrgConfig,
    #[serde(default)]
    pub repos: RepoConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GeneralConfig {
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: u64,
    #[serde(default = "default_view")]
    pub default_view: String,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct OrgConfig {
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct RepoConfig {
    #[serde(default)]
    pub exclude: Vec<String>,
}

fn default_general() -> GeneralConfig {
    GeneralConfig {
        refresh_interval_secs: 60,
        default_view: "repos".to_string(),
    }
}
fn default_refresh_interval() -> u64 {
    60
}
fn default_view() -> String {
    "repos".to_string()
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            Ok(toml::from_str(&content)?)
        } else {
            let config = Config {
                general: default_general(),
                orgs: OrgConfig::default(),
                repos: RepoConfig::default(),
            };
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, toml::to_string_pretty(&config)?)?;
            Ok(config)
        }
    }

    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("git-mux")
            .join("config.toml")
    }

    pub fn default_view(&self) -> &str {
        &self.general.default_view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.general.refresh_interval_secs, 60);
        assert_eq!(config.general.default_view, "repos");
        assert!(config.orgs.include.is_empty());
    }

    #[test]
    fn test_partial_config() {
        let config: Config = toml::from_str(
            r#"
            [general]
            refresh_interval_secs = 30
        "#,
        )
        .unwrap();
        assert_eq!(config.general.refresh_interval_secs, 30);
        assert_eq!(config.general.default_view, "repos");
    }

    #[test]
    fn test_repo_exclusion_config() {
        let config: Config = toml::from_str(
            r#"
            [repos]
            exclude = ["owner/repo-name", "org/internal-tool"]
        "#,
        )
        .unwrap();
        assert_eq!(config.repos.exclude.len(), 2);
        assert_eq!(config.repos.exclude[0], "owner/repo-name");
    }
}
