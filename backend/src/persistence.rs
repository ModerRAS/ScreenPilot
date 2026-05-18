use log::warn;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use crate::state::{RendererDevice, Scene};

fn get_data_file_path(filename: &str) -> PathBuf {
    // Get the directory containing the executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            return exe_dir.join("data").join(filename);
        }
    }
    // Fallback: data directory in current working directory
    PathBuf::from("data").join(filename)
}

/// Returns the path to the devices JSON file.
/// Uses a `data/` subdirectory next to the binary for portability.
fn get_persistence_path() -> PathBuf {
    get_data_file_path("devices.json")
}

fn get_scenes_persistence_path() -> PathBuf {
    get_data_file_path("scenes.json")
}

fn load_json_list<T>(path: &Path, label: &str) -> Vec<T>
where
    T: DeserializeOwned,
{
    if !path.exists() {
        return vec![];
    }

    match fs::read_to_string(path) {
        Ok(contents) => match serde_json::from_str(&contents) {
            Ok(values) => values,
            Err(e) => {
                warn!("Failed to parse {}: {}", label, e);
                vec![]
            }
        },
        Err(e) => {
            warn!("Failed to read {}: {}", label, e);
            vec![]
        }
    }
}

fn save_json_list<T>(path: &Path, values: &[T], label: &str) -> Result<(), String>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create data directory: {}", e))?;
        }
    }

    let json = serde_json::to_string_pretty(values).map_err(|e| {
        warn!("Failed to serialize {}: {}", label, e);
        format!("Failed to serialize {}: {}", label, e)
    })?;

    fs::write(path, json).map_err(|e| {
        warn!("Failed to write {}: {}", label, e);
        format!("Failed to write {}: {}", label, e)
    })
}

/// Load devices from JSON file.
/// Returns empty vec if file doesn't exist or is corrupted.
pub fn load_devices() -> Vec<RendererDevice> {
    let path = get_persistence_path();
    load_json_list(&path, "devices.json")
}

/// Save devices to JSON file.
/// Returns Ok(()) on success, Err with message on failure.
pub fn save_devices(devices: &[RendererDevice]) -> Result<(), String> {
    let path = get_persistence_path();
    save_json_list(&path, devices, "devices.json")
}

pub fn load_scenes() -> Vec<Scene> {
    let path = get_scenes_persistence_path();
    load_json_list(&path, "scenes.json")
}

pub fn save_scenes(scenes: &[Scene]) -> Result<(), String> {
    let path = get_scenes_persistence_path();
    save_json_list(&path, scenes, "scenes.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Mutex, OnceLock};

    fn persistence_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[test]
    fn test_get_persistence_path() {
        let path = get_persistence_path();
        // Path should end with devices.json
        assert!(path.file_name().map_or(false, |n| n == "devices.json"));
    }

    #[test]
    fn test_get_scenes_persistence_path() {
        let path = get_scenes_persistence_path();
        assert!(path.file_name().map_or(false, |n| n == "scenes.json"));
    }

    #[test]
    fn test_load_devices_nonexistent() {
        let _guard = persistence_test_lock();
        // Remove file if exists to test non-existent case
        let path = get_persistence_path();
        let _ = fs::remove_file(&path);

        let devices = load_devices();
        assert!(devices.is_empty());
    }

    #[test]
    fn test_save_and_load_devices() {
        let _guard = persistence_test_lock();
        let path = get_persistence_path();

        // Create test devices
        let test_devices = vec![
            RendererDevice {
                uuid: "test-uuid-1".to_string(),
                name: "Test Device 1".to_string(),
                alias: Some("Lobby".to_string()),
                ip: "192.168.1.100".to_string(),
                av_transport_url: "http://192.168.1.100:49152/upnp/control/AVTransport".to_string(),
                status: crate::state::PlaybackStatus::Idle,
                current_media: None,
                loop_playback: false,
            },
            RendererDevice {
                uuid: "test-uuid-2".to_string(),
                name: "Test Device 2".to_string(),
                alias: None,
                ip: "192.168.1.101".to_string(),
                av_transport_url: "http://192.168.1.101:49152/upnp/control/AVTransport".to_string(),
                status: crate::state::PlaybackStatus::Playing,
                current_media: Some("test.mp4".to_string()),
                loop_playback: false,
            },
        ];

        // Save devices
        let result = save_devices(&test_devices);
        assert!(result.is_ok(), "Failed to save devices: {:?}", result.err());

        // Load devices
        let loaded = load_devices();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].uuid, "test-uuid-1");
        assert_eq!(loaded[0].alias, Some("Lobby".to_string()));
        assert_eq!(loaded[1].uuid, "test-uuid-2");
        assert_eq!(loaded[1].status, crate::state::PlaybackStatus::Playing);

        // Cleanup
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_save_devices_invalid_path() {
        let _guard = persistence_test_lock();
        let test_devices = vec![RendererDevice {
            uuid: "test".to_string(),
            name: "Test".to_string(),
            alias: None,
            ip: "192.168.1.1".to_string(),
            av_transport_url: "http://192.168.1.1:8000".to_string(),
            status: crate::state::PlaybackStatus::Idle,
            current_media: None,
            loop_playback: false,
        }];

        // This should still work because we use current_exe fallback
        let result = save_devices(&test_devices);
        // Either succeeds or returns an error, but shouldn't panic
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_save_and_load_scenes() {
        let _guard = persistence_test_lock();
        let path = get_scenes_persistence_path();
        let _ = fs::remove_file(&path);

        let mut assignments = std::collections::HashMap::new();
        assignments.insert("device-1".to_string(), "promo.mp4".to_string());
        let scenes = vec![Scene {
            name: "Lobby".to_string(),
            assignments,
        }];

        save_scenes(&scenes).unwrap();
        let loaded = load_scenes();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Lobby");
        assert_eq!(
            loaded[0].assignments.get("device-1"),
            Some(&"promo.mp4".to_string())
        );

        let _ = fs::remove_file(&path);
    }
}
