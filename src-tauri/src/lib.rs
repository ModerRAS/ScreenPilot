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

