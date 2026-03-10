// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::time::Duration;

use screen_pilot_lib::{
    TauriAppState, SceneApplyResult,
    discovery, dlna, media_server, state,
};
use screen_pilot_lib::state::{PlaybackStatus, RendererDevice, Scene};
use tauri::State;
use tokio::sync::Mutex;
use reqwest::Client;

// ─── Tauri Commands ───────────────────────────────────────────────────────────

/// Trigger a fresh SSDP discovery scan and return the updated device list.
#[tauri::command]
async fn discover_devices(
    app_state: State<'_, TauriAppState>,
) -> Result<Vec<RendererDevice>, String> {
    let devices = discovery::discover_renderers().await;
    let mut st = app_state.shared.write().await;

    let existing: HashMap<String, (PlaybackStatus, Option<String>)> = st
        .devices
        .iter()
        .map(|d| (d.uuid.clone(), (d.status.clone(), d.current_media.clone())))
        .collect();

    let mut merged: Vec<RendererDevice> = devices
        .into_iter()
        .map(|mut d| {
            if let Some((status, media)) = existing.get(&d.uuid) {
                d.status = status.clone();
                d.current_media = media.clone();
            }
            d
        })
        .collect();

    for old in &st.devices {
        if !merged.iter().any(|d| d.uuid == old.uuid) {
            merged.push(old.clone());
        }
    }

    st.devices = merged.clone();
    Ok(merged)
}

/// Return the current device list without triggering a new scan.
#[tauri::command]
async fn get_devices(
    app_state: State<'_, TauriAppState>,
) -> Result<Vec<RendererDevice>, String> {
    let st = app_state.shared.read().await;
    Ok(st.devices.clone())
}

/// Play a media file on a specific device.
#[tauri::command]
async fn play_on_device(
    device_uuid: String,
    media_filename: String,
    app_state: State<'_, TauriAppState>,
) -> Result<(), String> {
    let (av_url, media_uri) = {
        let st = app_state.shared.read().await;
        let device = st
            .devices
            .iter()
            .find(|d| d.uuid == device_uuid)
            .ok_or_else(|| format!("Device not found: {}", device_uuid))?;
        let uri = format!("{}/media/{}", st.media_server_base_url, media_filename);
        (device.av_transport_url.clone(), uri)
    };

    let client = app_state.client.lock().await;
    dlna::play_media(&client, &av_url, &media_uri)
        .await
        .map_err(|e| e.to_string())?;

    let mut st = app_state.shared.write().await;
    if let Some(device) = st.devices.iter_mut().find(|d| d.uuid == device_uuid) {
        device.status = PlaybackStatus::Playing;
        device.current_media = Some(media_filename);
    }
    Ok(())
}

/// Pause playback on a specific device.
#[tauri::command]
async fn pause_device(
    device_uuid: String,
    app_state: State<'_, TauriAppState>,
) -> Result<(), String> {
    let av_url = resolve_av_url(&app_state, &device_uuid).await?;
    let client = app_state.client.lock().await;
    dlna::pause(&client, &av_url).await.map_err(|e| e.to_string())?;

    let mut st = app_state.shared.write().await;
    if let Some(d) = st.devices.iter_mut().find(|d| d.uuid == device_uuid) {
        d.status = PlaybackStatus::Paused;
    }
    Ok(())
}

/// Stop playback on a specific device.
#[tauri::command]
async fn stop_device(
    device_uuid: String,
    app_state: State<'_, TauriAppState>,
) -> Result<(), String> {
    let av_url = resolve_av_url(&app_state, &device_uuid).await?;
    let client = app_state.client.lock().await;
    dlna::stop(&client, &av_url).await.map_err(|e| e.to_string())?;

    let mut st = app_state.shared.write().await;
    if let Some(d) = st.devices.iter_mut().find(|d| d.uuid == device_uuid) {
        d.status = PlaybackStatus::Stopped;
    }
    Ok(())
}

/// List media files in the media directory.
#[tauri::command]
async fn list_media(
    app_state: State<'_, TauriAppState>,
) -> Result<Vec<String>, String> {
    Ok(media_server::list_media_files(&app_state.media_dir))
}

/// Return the list of defined scenes.
#[tauri::command]
async fn get_scenes(
    app_state: State<'_, TauriAppState>,
) -> Result<Vec<Scene>, String> {
    let st = app_state.shared.read().await;
    Ok(st.scenes.clone())
}

/// Save (create or update) a scene.
#[tauri::command]
async fn save_scene(
    scene: Scene,
    app_state: State<'_, TauriAppState>,
) -> Result<(), String> {
    let mut st = app_state.shared.write().await;
    if let Some(existing) = st.scenes.iter_mut().find(|s| s.name == scene.name) {
        *existing = scene;
    } else {
        st.scenes.push(scene);
    }
    Ok(())
}

/// Delete a scene by name.
#[tauri::command]
async fn delete_scene(
    scene_name: String,
    app_state: State<'_, TauriAppState>,
) -> Result<(), String> {
    let mut st = app_state.shared.write().await;
    st.scenes.retain(|s| s.name != scene_name);
    Ok(())
}

/// Apply a scene: send the correct play command to each assigned device.
#[tauri::command]
async fn apply_scene(
    scene_name: String,
    app_state: State<'_, TauriAppState>,
) -> Result<Vec<SceneApplyResult>, String> {
    let (assignments, media_base) = {
        let st = app_state.shared.read().await;
        let scene = st
            .scenes
            .iter()
            .find(|s| s.name == scene_name)
            .ok_or_else(|| format!("Scene not found: {}", scene_name))?;
        (scene.assignments.clone(), st.media_server_base_url.clone())
    };

    let mut results = Vec::new();
    for (uuid, filename) in &assignments {
        let av_url = match resolve_av_url(&app_state, uuid).await {
            Ok(u) => u,
            Err(e) => {
                results.push(SceneApplyResult { device_uuid: uuid.clone(), success: false, error: Some(e) });
                continue;
            }
        };
        let media_uri = format!("{}/media/{}", media_base, filename);
        let client = app_state.client.lock().await;
        match dlna::play_media(&client, &av_url, &media_uri).await {
            Ok(_) => {
                drop(client);
                let mut st = app_state.shared.write().await;
                if let Some(d) = st.devices.iter_mut().find(|d| d.uuid == *uuid) {
                    d.status = PlaybackStatus::Playing;
                    d.current_media = Some(filename.clone());
                }
                results.push(SceneApplyResult { device_uuid: uuid.clone(), success: true, error: None });
            }
            Err(e) => {
                results.push(SceneApplyResult { device_uuid: uuid.clone(), success: false, error: Some(e.to_string()) });
            }
        }
    }
    Ok(results)
}

/// Get the media server base URL.
#[tauri::command]
async fn get_media_server_url(
    app_state: State<'_, TauriAppState>,
) -> Result<String, String> {
    let st = app_state.shared.read().await;
    Ok(st.media_server_base_url.clone())
}

// ─── Helper ───────────────────────────────────────────────────────────────────

async fn resolve_av_url(app_state: &TauriAppState, uuid: &str) -> Result<String, String> {
    let st = app_state.shared.read().await;
    st.devices
        .iter()
        .find(|d| d.uuid == uuid)
        .map(|d| d.av_transport_url.clone())
        .ok_or_else(|| format!("Device not found: {}", uuid))
}

// ─── Entry point ──────────────────────────────────────────────────────────────

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");

    let media_dir = screen_pilot_lib::resolve_media_dir();

    let (_, media_base_url) = rt
        .block_on(media_server::start_media_server(media_dir.clone(), 8090))
        .expect("Failed to start media server");

    let shared = state::new_shared_state();
    rt.block_on(async {
        let mut s = shared.write().await;
        s.media_server_base_url = media_base_url;
    });

    let shared_for_bg = shared.clone();
    rt.spawn(async move {
        loop {
            let devices = discovery::discover_renderers().await;
            let mut st = shared_for_bg.write().await;
            for d in &mut st.devices {
                if let Some(fresh) = devices.iter().find(|f| f.uuid == d.uuid) {
                    d.name = fresh.name.clone();
                    d.ip = fresh.ip.clone();
                    d.av_transport_url = fresh.av_transport_url.clone();
                }
            }
            for fresh in &devices {
                if !st.devices.iter().any(|d| d.uuid == fresh.uuid) {
                    st.devices.push(fresh.clone());
                }
            }
            drop(st);
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(TauriAppState {
            shared,
            client: Mutex::new(Client::new()),
            media_dir,
        })
        .invoke_handler(tauri::generate_handler![
            discover_devices,
            get_devices,
            play_on_device,
            pause_device,
            stop_device,
            list_media,
            get_scenes,
            save_scene,
            delete_scene,
            apply_scene,
            get_media_server_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
