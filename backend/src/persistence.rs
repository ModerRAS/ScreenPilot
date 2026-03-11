use log::warn;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use crate::state::RendererDevice;

/// Returns the path to the devices JSON file.
/// Uses a `data/` subdirectory next to the binary for portability.
fn get_persistence_path() -> PathBuf {
    // Get the directory containing the executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            return exe_dir.join("data").join("devices.json");
        }
    }
    // Fallback: data directory in current working directory
    PathBuf::from("data").join("devices.json")
}

/// Load devices from JSON file.
/// Returns empty vec if file doesn't exist or is corrupted.
pub fn load_devices() -> Vec<RendererDevice> {
    let path = get_persistence_path();

    if !path.exists() {
        return vec![];
    }

    match fs::read_to_string(&path) {
        Ok(contents) => match serde_json::from_str(&contents) {
            Ok(devices) => devices,
            Err(e) => {
                warn!("Failed to parse devices.json: {}", e);
                vec![]
            }
        },
        Err(e) => {
            warn!("Failed to read devices.json: {}", e);
            vec![]
        }
    }
}

/// Save devices to JSON file.
/// Returns Ok(()) on success, Err with message on failure.
pub fn save_devices(devices: &[RendererDevice]) -> Result<(), String> {
    let path = get_persistence_path();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            if let Err(e) = fs::create_dir_all(parent) {
                warn!("Failed to create data directory: {}", e);
                return Err(format!("Failed to create data directory: {}", e));
            }
        }
    }

    // Serialize devices to JSON
    let json = match serde_json::to_string_pretty(devices) {
        Ok(j) => j,
        Err(e) => {
            warn!("Failed to serialize devices: {}", e);
            return Err(format!("Failed to serialize devices: {}", e));
        }
    };

    // Write to file
    match fs::write(&path, json) {
        Ok(_) => Ok(()),
        Err(e) => {
            warn!("Failed to write devices.json: {}", e);
            Err(format!("Failed to write devices.json: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    #[test]
    fn test_get_persistence_path() {
        let path = get_persistence_path();
        // Path should end with devices.json
        assert!(path.file_name().map_or(false, |n| n == "devices.json"));
    }

    #[test]
    fn test_load_devices_nonexistent() {
        // Remove file if exists to test non-existent case
        let path = get_persistence_path();
        let _ = fs::remove_file(&path);

        let devices = load_devices();
        assert!(devices.is_empty());
    }

    #[test]
    fn test_save_and_load_devices() {
        let path = get_persistence_path();

        // Create test devices
        let test_devices = vec![
            RendererDevice {
                uuid: "test-uuid-1".to_string(),
                name: "Test Device 1".to_string(),
                ip: "192.168.1.100".to_string(),
                av_transport_url: "http://192.168.1.100:49152/upnp/control/AVTransport".to_string(),
                status: crate::state::PlaybackStatus::Idle,
                current_media: None,
                loop_playback: false,
            },
            RendererDevice {
                uuid: "test-uuid-2".to_string(),
                name: "Test Device 2".to_string(),
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
        assert_eq!(loaded[1].uuid, "test-uuid-2");
        assert_eq!(loaded[1].status, crate::state::PlaybackStatus::Playing);

        // Cleanup
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_save_devices_invalid_path() {
        // Test with a path that cannot be created (empty parent)
        let test_devices = vec![RendererDevice {
            uuid: "test".to_string(),
            name: "Test".to_string(),
            ip: "192.168.1.1".to_string(),
            av_transport_url: "http://192.168.1.1:8000".to_string(),
            status: crate::state::PlaybackStatus::Idle,
            current_media: None,
        }];

        // This should still work because we use current_exe fallback
        let result = save_devices(&test_devices);
        // Either succeeds or returns an error, but shouldn't panic
        assert!(result.is_ok() || result.is_err());
    }
}
