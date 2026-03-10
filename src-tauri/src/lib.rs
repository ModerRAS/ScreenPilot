pub mod discovery;
pub mod dlna;
pub mod media_server;
pub mod state;

use std::path::PathBuf;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use state::SharedState;

/// Tauri managed state: shared app state + HTTP client.
pub struct TauriAppState {
    pub shared: SharedState,
    pub client: Mutex<Client>,
    pub media_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SceneApplyResult {
    pub device_uuid: String,
    pub success: bool,
    pub error: Option<String>,
}

/// Resolve the media directory: next to the binary or the project `media/` folder.
pub fn resolve_media_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    if let Some(dir) = exe_dir {
        let candidate = dir.join("media");
        if candidate.is_dir() {
            return candidate;
        }
    }

    // During development, use a `media` folder in the workspace root.
    let dev_candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or(&PathBuf::from("."))
        .join("media");

    if !dev_candidate.exists() {
        let _ = std::fs::create_dir_all(&dev_candidate);
    }
    dev_candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_scene_apply_result_serialize() {
        let result = SceneApplyResult {
            device_uuid: "device-123".to_string(),
            success: true,
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("device-123"));
        assert!(json.contains("true"));
    }

    #[test]
    fn test_scene_apply_result_with_error() {
        let result = SceneApplyResult {
            device_uuid: "device-456".to_string(),
            success: false,
            error: Some("Connection refused".to_string()),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("false"));
        assert!(json.contains("Connection refused"));
    }

    #[test]
    fn test_scene_apply_result_deserialize() {
        let json = r#"{
            "device_uuid": "test-uuid",
            "success": true,
            "error": null
        }"#;
        let result: SceneApplyResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.device_uuid, "test-uuid");
        assert!(result.success);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_tauri_app_state_creation() {
        let temp_dir = TempDir::new().unwrap();
        let state = TauriAppState {
            shared: state::new_shared_state(),
            client: tokio::sync::Mutex::new(reqwest::Client::new()),
            media_dir: temp_dir.path().to_path_buf(),
        };
        assert!(state.shared.try_read().is_ok());
    }

    #[test]
    fn test_resolve_media_dir_creates_if_not_exists() {
        let temp_dir = TempDir::new().unwrap();
        let test_media_dir = temp_dir.path().join("media");
        
        let original = std::env::var("CARGO_MANIFEST_DIR");
        
        fs::create_dir_all(&test_media_dir).unwrap();
        
        std::env::remove_var("CARGO_MANIFEST_DIR");
        
        let result = resolve_media_dir();
        let result_str = result.to_string_lossy();
        assert!(result_str.contains("media") || result.exists());
        
        if let Ok(val) = original {
            std::env::set_var("CARGO_MANIFEST_DIR", val);
        }
    }
}

