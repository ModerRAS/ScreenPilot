use log::warn;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: AppConfig,
    pub path: PathBuf,
    pub created: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub auth: AuthSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSettings {
    pub username: String,
    pub password: String,
}

impl AppConfig {
    fn default_with_password(password: String) -> Self {
        Self {
            auth: AuthSettings {
                username: "admin".to_string(),
                password,
            },
        }
    }

    fn validate(mut self) -> Result<Self, String> {
        self.auth.username = self.auth.username.trim().to_string();

        if self.auth.username.is_empty() {
            return Err("auth.username must not be empty".to_string());
        }

        if self.auth.password.is_empty() {
            return Err("auth.password must not be empty".to_string());
        }

        Ok(self)
    }
}

pub fn get_config_path() -> PathBuf {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            return exe_dir.join("data").join("config.toml");
        }
    }

    PathBuf::from("data").join("config.toml")
}

pub fn load_or_create_config(generated_password: String) -> Result<LoadedConfig, String> {
    load_or_create_config_at(get_config_path(), generated_password)
}

fn load_or_create_config_at(
    path: PathBuf,
    generated_password: String,
) -> Result<LoadedConfig, String> {
    if path.exists() {
        let contents =
            fs::read_to_string(&path).map_err(|e| format!("Failed to read config.toml: {}", e))?;
        let config: AppConfig =
            toml::from_str(&contents).map_err(|e| format!("Failed to parse config.toml: {}", e))?;
        return Ok(LoadedConfig {
            config: config.validate()?,
            path,
            created: false,
        });
    }

    let config = AppConfig::default_with_password(generated_password).validate()?;
    save_config_at(&path, &config)?;

    Ok(LoadedConfig {
        config,
        path,
        created: true,
    })
}

fn save_config_at(path: &Path, config: &AppConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }
    }

    let toml = toml::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config.toml: {}", e))?;
    fs::write(path, toml).map_err(|e| {
        warn!("Failed to write config.toml: {}", e);
        format!("Failed to write config.toml: {}", e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_or_create_config_creates_default_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("config.toml");

        let loaded = load_or_create_config_at(path.clone(), "secret123".to_string()).unwrap();

        assert!(loaded.created);
        assert_eq!(loaded.path, path);
        assert_eq!(loaded.config.auth.username, "admin");
        assert_eq!(loaded.config.auth.password, "secret123");
        assert!(loaded.path.exists());
    }

    #[test]
    fn test_load_or_create_config_reads_existing_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("config.toml");
        fs::write(
            &path,
            r#"[auth]
username = "operator"
password = "configured"
"#,
        )
        .unwrap();

        let loaded = load_or_create_config_at(path, "ignored".to_string()).unwrap();

        assert!(!loaded.created);
        assert_eq!(loaded.config.auth.username, "operator");
        assert_eq!(loaded.config.auth.password, "configured");
    }

    #[test]
    fn test_load_or_create_config_rejects_empty_password() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("config.toml");
        fs::write(
            &path,
            r#"[auth]
username = "operator"
password = ""
"#,
        )
        .unwrap();

        let err = load_or_create_config_at(path, "ignored".to_string()).unwrap_err();
        assert!(err.contains("auth.password"));
    }
}
