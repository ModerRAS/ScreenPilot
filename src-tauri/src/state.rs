use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Represents a discovered DLNA MediaRenderer device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendererDevice {
    pub uuid: String,
    pub name: String,
    pub ip: String,
    pub av_transport_url: String,
    pub status: PlaybackStatus,
    pub current_media: Option<String>,
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
}

pub type SharedState = Arc<RwLock<AppState>>;

pub fn new_shared_state() -> SharedState {
    Arc::new(RwLock::new(AppState::default()))
}
