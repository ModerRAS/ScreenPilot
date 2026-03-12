use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// Re-export encoder types for use in AppState
pub use crate::encoder::DetectionResult;

/// Represents a discovered DLNA MediaRenderer device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendererDevice {
    pub uuid: String,
    pub name: String,
    pub ip: String,
    pub av_transport_url: String,
    pub status: PlaybackStatus,
    pub current_media: Option<String>,
    pub loop_playback: bool,
}

/// Current playback status of a renderer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PlaybackStatus {
    Idle,
    Playing,
    Paused,
    Stopped,
    Error,
}

impl std::fmt::Display for PlaybackStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlaybackStatus::Idle => write!(f, "Idle"),
            PlaybackStatus::Playing => write!(f, "Playing"),
            PlaybackStatus::Paused => write!(f, "Paused"),
            PlaybackStatus::Stopped => write!(f, "Stopped"),
            PlaybackStatus::Error => write!(f, "Error"),
        }
    }
}

/// A scene maps each device UUID to a media filename.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub name: String,
    /// device uuid -> media filename (e.g. "ad.mp4")
    pub assignments: HashMap<String, String>,
}

/// Shared application state, wrapped in Arc<RwLock<…>> for async access.
#[derive(Debug, Default)]
pub struct AppState {
    pub devices: Vec<RendererDevice>,
    pub scenes: Vec<Scene>,
    /// Base URL for the media server, e.g. "http://192.168.1.10:8090"
    pub media_server_base_url: String,
    /// Preferred encoder: "auto", "nvidia", "amd", "intel", "apple", "vaapi", "software"
    pub preferred_encoder: String,
    /// Loop playback for transcoded stream
    pub loop_playback: bool,
    /// Cached hardware encoder detection result
    pub detected_encoders: Option<DetectionResult>,
}

pub type SharedState = Arc<RwLock<AppState>>;

pub fn new_shared_state() -> SharedState {
    Arc::new(RwLock::new(AppState::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_renderer_device_creation() {
        let device = RendererDevice {
            uuid: "test-uuid-123".to_string(),
            name: "Living Room TV".to_string(),
            ip: "192.168.1.100".to_string(),
            av_transport_url: "http://192.168.1.100:49152/upnp/control/AVTransport".to_string(),
            status: PlaybackStatus::Idle,
            current_media: None,
            loop_playback: false,
        };
        assert_eq!(device.uuid, "test-uuid-123");
        assert_eq!(device.name, "Living Room TV");
        assert_eq!(device.status, PlaybackStatus::Idle);
        assert!(device.current_media.is_none());
    }

    #[test]
    fn test_renderer_device_clone() {
        let device1 = RendererDevice {
            uuid: "uuid-1".to_string(),
            name: "Device 1".to_string(),
            ip: "192.168.1.1".to_string(),
            av_transport_url: "http://192.168.1.1:8008/ctrl".to_string(),
            status: PlaybackStatus::Playing,
            current_media: Some("ad.mp4".to_string()),
            loop_playback: false,
        };
        let device2 = device1.clone();
        assert_eq!(device1.uuid, device2.uuid);
        assert_eq!(device1.name, device2.name);
        assert_eq!(device1.status, device2.status);
        assert_eq!(device1.current_media, device2.current_media);
    }

    #[test]
    fn test_playback_status_display() {
        assert_eq!(PlaybackStatus::Idle.to_string(), "Idle");
        assert_eq!(PlaybackStatus::Playing.to_string(), "Playing");
        assert_eq!(PlaybackStatus::Paused.to_string(), "Paused");
        assert_eq!(PlaybackStatus::Stopped.to_string(), "Stopped");
        assert_eq!(PlaybackStatus::Error.to_string(), "Error");
    }

    #[test]
    fn test_playback_status_serialize() {
        let json = serde_json::to_string(&PlaybackStatus::Playing).unwrap();
        assert!(json.contains("playing"));
    }

    #[test]
    fn test_playback_status_deserialize() {
        let status: PlaybackStatus = serde_json::from_str("\"paused\"").unwrap();
        assert_eq!(status, PlaybackStatus::Paused);
    }

    #[test]
    fn test_scene_creation() {
        let mut assignments = HashMap::new();
        assignments.insert("device-uuid-1".to_string(), "ad.mp4".to_string());
        assignments.insert("device-uuid-2".to_string(), "promo.mp4".to_string());

        let scene = Scene {
            name: "Morning Scene".to_string(),
            assignments,
        };

        assert_eq!(scene.name, "Morning Scene");
        assert_eq!(scene.assignments.len(), 2);
    }

    #[test]
    fn test_scene_serialize() {
        let mut assignments = HashMap::new();
        assignments.insert("uuid-1".to_string(), "video1.mp4".to_string());

        let scene = Scene {
            name: "Test Scene".to_string(),
            assignments,
        };

        let json = serde_json::to_string(&scene).unwrap();
        assert!(json.contains("Test Scene"));
        assert!(json.contains("uuid-1"));
    }

    #[test]
    fn test_scene_deserialize() {
        let json = r#"{
            "name": "Evening Scene",
            "assignments": {
                "tv-uuid": "movie.mp4",
                "projector-uuid": "slides.mp4"
            }
        }"#;

        let scene: Scene = serde_json::from_str(json).unwrap();
        assert_eq!(scene.name, "Evening Scene");
        assert_eq!(scene.assignments.get("tv-uuid"), Some(&"movie.mp4".to_string()));
    }

    #[test]
    fn test_app_state_default() {
        let state = AppState::default();
        assert!(state.devices.is_empty());
        assert!(state.scenes.is_empty());
        assert!(state.media_server_base_url.is_empty());
    }

    #[test]
    fn test_app_state_with_data() {
        let device = RendererDevice {
            uuid: "test".to_string(),
            name: "Test".to_string(),
            ip: "192.168.1.1".to_string(),
            av_transport_url: "http://192.168.1.1:8000".to_string(),
            status: PlaybackStatus::Idle,
            current_media: None,
            loop_playback: false,
        };

        let mut assignments = HashMap::new();
        assignments.insert("test-uuid".to_string(), "test.mp4".to_string());
        let scene = Scene {
            name: "Test".to_string(),
            assignments,
        };

        let state = AppState {
            devices: vec![device],
            scenes: vec![scene],
            media_server_base_url: "http://192.168.1.10:8090".to_string(),
            preferred_encoder: "auto".to_string(),
            loop_playback: true,
            detected_encoders: None,
        };

        assert_eq!(state.devices.len(), 1);
        assert_eq!(state.scenes.len(), 1);
        assert_eq!(state.media_server_base_url, "http://192.168.1.10:8090");
    }

    #[test]
    fn test_new_shared_state() {
        let state = new_shared_state();
        let read_state = state.try_read().unwrap();
        assert!(read_state.devices.is_empty());
        assert!(read_state.scenes.is_empty());
    }

    #[test]
    fn test_shared_state_concurrent_access() {
        use std::sync::Arc;
        use tokio::sync::RwLock;

        let state: SharedState = Arc::new(RwLock::new(AppState::default()));

        let state_clone = state.clone();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            {
                let mut write_state = state_clone.write().await;
                write_state.media_server_base_url = "http://test:8080".to_string();
            }
            
            let read_state = state_clone.read().await;
            assert_eq!(read_state.media_server_base_url, "http://test:8080");
        });
    }
}
