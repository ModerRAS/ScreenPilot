mod discovery;
mod dlna;
mod frontend;
mod media_server;
mod state;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

use state::{PlaybackStatus, RendererDevice, Scene, SharedState};
use frontend::Frontend;

async fn serve_frontend() -> impl axum::response::IntoResponse {
    let html = Frontend::get("index.html")
        .map(|f| String::from_utf8_lossy(f.data.as_ref()).to_string())
        .unwrap_or_else(|| "Frontend not found".to_string());
    axum::response::Html(html)
}

async fn serve_assets(Path(path): Path<String>) -> impl axum::response::IntoResponse {
    let full_path = format!("assets/{}", path);
    match Frontend::get(&full_path) {
        Some(file) => {
            let mime = match path.rsplit('.').next() {
                Some("js") => "application/javascript",
                Some("css") => "text/css",
                Some("html") => "text/html",
                Some("ico") => "image/x-icon",
                Some("png") => "image/png",
                Some("jpg") | Some("jpeg") => "image/jpeg",
                Some("svg") => "image/svg+xml",
                Some("woff") => "font/woff",
                Some("woff2") => "font/woff2",
                _ => "application/octet-stream",
            };
            let body = axum::body::Body::from(file.data.as_ref().to_vec());
            axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", mime)
                .body(body)
                .unwrap()
        }
        None => axum::response::Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body("Not found".into())
            .unwrap(),
    }
}

async fn serve_favicon() -> impl axum::response::IntoResponse {
    match Frontend::get("favicon.ico") {
        Some(file) => {
            let body = axum::body::Body::from(file.data.as_ref().to_vec());
            axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "image/x-icon")
                .body(body)
                .unwrap()
        }
        None => axum::response::Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body("Not found".into())
            .unwrap(),
    }
}

// ─── Shared application state for Axum ────────────────────────────────────────

#[derive(Clone)]
pub struct WebAppState {
    pub shared: SharedState,
    pub client: Arc<Mutex<Client>>,
    pub media_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SceneApplyResult {
    pub device_uuid: String,
    pub success: bool,
    pub error: Option<String>,
}

// ─── Request / Response types ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct PlayRequest {
    media_filename: String,
}

#[derive(Deserialize)]
struct SaveSceneRequest {
    name: String,
    assignments: HashMap<String, String>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

type ApiError = (StatusCode, Json<ErrorResponse>);

fn error_response(status: StatusCode, msg: impl Into<String>) -> ApiError {
    (status, Json(ErrorResponse { error: msg.into() }))
}

/// Validate that a media filename is safe (no path traversal).
fn validate_media_filename(filename: &str) -> Result<(), ApiError> {
    if filename.is_empty()
        || filename.contains('/')
        || filename.contains('\\')
        || filename.contains("..")
        || filename.contains('\0')
    {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "Invalid media filename",
        ));
    }
    Ok(())
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// GET /api/devices — return the current device list without triggering a new scan.
async fn get_devices(State(app): State<WebAppState>) -> Json<Vec<RendererDevice>> {
    let st = app.shared.read().await;
    Json(st.devices.clone())
}

/// POST /api/devices/discover — trigger SSDP discovery and return updated list.
async fn discover_devices(State(app): State<WebAppState>) -> Json<Vec<RendererDevice>> {
    let devices = discovery::discover_renderers().await;
    let mut st = app.shared.write().await;

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
    Json(merged)
}

/// POST /api/devices/:uuid/play — play a media file on a specific device.
async fn play_on_device(
    State(app): State<WebAppState>,
    Path(device_uuid): Path<String>,
    Json(body): Json<PlayRequest>,
) -> Result<StatusCode, ApiError> {
    validate_media_filename(&body.media_filename)?;

    let (av_url, media_uri) = {
        let st = app.shared.read().await;
        let device = st
            .devices
            .iter()
            .find(|d| d.uuid == device_uuid)
            .ok_or_else(|| {
                error_response(
                    StatusCode::NOT_FOUND,
                    format!("Device not found: {}", device_uuid),
                )
            })?;
        let uri = format!("{}/media/{}", st.media_server_base_url, body.media_filename);
        (device.av_transport_url.clone(), uri)
    };

    let client = app.client.lock().await;
    dlna::play_media(&client, &av_url, &media_uri)
        .await
        .map_err(|e| error_response(StatusCode::BAD_GATEWAY, e.to_string()))?;

    let mut st = app.shared.write().await;
    if let Some(device) = st.devices.iter_mut().find(|d| d.uuid == device_uuid) {
        device.status = PlaybackStatus::Playing;
        device.current_media = Some(body.media_filename);
    }
    Ok(StatusCode::OK)
}

/// POST /api/devices/:uuid/pause — pause playback on a specific device.
async fn pause_device(
    State(app): State<WebAppState>,
    Path(device_uuid): Path<String>,
) -> Result<StatusCode, ApiError> {
    let av_url = resolve_av_url(&app, &device_uuid).await.map_err(|e| {
        error_response(StatusCode::NOT_FOUND, e)
    })?;
    let client = app.client.lock().await;
    dlna::pause(&client, &av_url)
        .await
        .map_err(|e| error_response(StatusCode::BAD_GATEWAY, e.to_string()))?;

    let mut st = app.shared.write().await;
    if let Some(d) = st.devices.iter_mut().find(|d| d.uuid == device_uuid) {
        d.status = PlaybackStatus::Paused;
    }
    Ok(StatusCode::OK)
}

/// POST /api/devices/:uuid/stop — stop playback on a specific device.
async fn stop_device(
    State(app): State<WebAppState>,
    Path(device_uuid): Path<String>,
) -> Result<StatusCode, ApiError> {
    let av_url = resolve_av_url(&app, &device_uuid).await.map_err(|e| {
        error_response(StatusCode::NOT_FOUND, e)
    })?;
    let client = app.client.lock().await;
    dlna::stop(&client, &av_url)
        .await
        .map_err(|e| error_response(StatusCode::BAD_GATEWAY, e.to_string()))?;

    let mut st = app.shared.write().await;
    if let Some(d) = st.devices.iter_mut().find(|d| d.uuid == device_uuid) {
        d.status = PlaybackStatus::Stopped;
    }
    Ok(StatusCode::OK)
}

/// GET /api/media — list available media files.
async fn list_media(State(app): State<WebAppState>) -> Json<Vec<String>> {
    Json(media_server::list_media_files(&app.media_dir))
}

/// POST /api/media/upload — upload a media file.
async fn upload_media(
    State(app): State<WebAppState>,
    mut multipart: Multipart,
) -> Result<StatusCode, ApiError> {
    const ALLOWED_EXTENSIONS: &[&str] = &["mp4", "webm", "avi", "mkv", "mov"];

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        error_response(StatusCode::BAD_REQUEST, format!("Failed to parse multipart: {}", e))
    })? {
        let field_name = field.name().unwrap_or("unknown");

        if field_name == "file" {
            let filename = field.file_name().ok_or_else(|| {
                error_response(StatusCode::BAD_REQUEST, "No filename provided")
            })?.to_string();

            validate_media_filename(&filename)?;

            let ext = filename
                .rsplit('.')
                .next()
                .map(|e| e.to_lowercase())
                .ok_or_else(|| {
                    error_response(StatusCode::BAD_REQUEST, "No file extension")
                })?;

            if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
                return Err(error_response(
                    StatusCode::BAD_REQUEST,
                    format!("Invalid file extension '{}'. Allowed: {:?}", ext, ALLOWED_EXTENSIONS),
                ));
            }

            let data = field.bytes().await.map_err(|e| {
                error_response(StatusCode::BAD_REQUEST, format!("Failed to read file: {}", e))
            })?;

            let dest_path = app.media_dir.join(&filename);
            tokio::fs::write(&dest_path, data).await.map_err(|e| {
                error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save file: {}", e))
            })?;

            return Ok(StatusCode::OK);
        }
    }

    Err(error_response(StatusCode::BAD_REQUEST, "No file field in multipart form"))
}

/// GET /api/scenes — return the list of defined scenes.
async fn get_scenes(State(app): State<WebAppState>) -> Json<Vec<Scene>> {
    let st = app.shared.read().await;
    Json(st.scenes.clone())
}

/// POST /api/scenes — save (create or update) a scene.
async fn save_scene(
    State(app): State<WebAppState>,
    Json(body): Json<SaveSceneRequest>,
) -> Result<StatusCode, ApiError> {
    for filename in body.assignments.values() {
        validate_media_filename(filename)?;
    }
    let scene = Scene {
        name: body.name,
        assignments: body.assignments,
    };
    let mut st = app.shared.write().await;
    if let Some(existing) = st.scenes.iter_mut().find(|s| s.name == scene.name) {
        *existing = scene;
    } else {
        st.scenes.push(scene);
    }
    Ok(StatusCode::OK)
}

/// DELETE /api/scenes/:name — delete a scene by name.
async fn delete_scene(
    State(app): State<WebAppState>,
    Path(scene_name): Path<String>,
) -> StatusCode {
    let mut st = app.shared.write().await;
    st.scenes.retain(|s| s.name != scene_name);
    StatusCode::OK
}

/// POST /api/scenes/:name/apply — apply a scene to all assigned devices.
async fn apply_scene(
    State(app): State<WebAppState>,
    Path(scene_name): Path<String>,
) -> Result<Json<Vec<SceneApplyResult>>, ApiError> {
    let (assignments, media_base) = {
        let st = app.shared.read().await;
        let scene = st
            .scenes
            .iter()
            .find(|s| s.name == scene_name)
            .ok_or_else(|| {
                error_response(
                    StatusCode::NOT_FOUND,
                    format!("Scene not found: {}", scene_name),
                )
            })?;
        (scene.assignments.clone(), st.media_server_base_url.clone())
    };

    let mut results = Vec::new();
    for (uuid, filename) in &assignments {
        let av_url = match resolve_av_url(&app, uuid).await {
            Ok(u) => u,
            Err(e) => {
                results.push(SceneApplyResult {
                    device_uuid: uuid.clone(),
                    success: false,
                    error: Some(e),
                });
                continue;
            }
        };
        let media_uri = format!("{}/media/{}", media_base, filename);
        let client = app.client.lock().await;
        match dlna::play_media(&client, &av_url, &media_uri).await {
            Ok(_) => {
                drop(client);
                let mut st = app.shared.write().await;
                if let Some(d) = st.devices.iter_mut().find(|d| d.uuid == *uuid) {
                    d.status = PlaybackStatus::Playing;
                    d.current_media = Some(filename.clone());
                }
                results.push(SceneApplyResult {
                    device_uuid: uuid.clone(),
                    success: true,
                    error: None,
                });
            }
            Err(e) => {
                results.push(SceneApplyResult {
                    device_uuid: uuid.clone(),
                    success: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }
    Ok(Json(results))
}

/// GET /api/config/media-server-url — return the media server base URL.
async fn get_media_server_url(State(app): State<WebAppState>) -> Json<String> {
    let st = app.shared.read().await;
    Json(st.media_server_base_url.clone())
}

// ─── Helper ───────────────────────────────────────────────────────────────────

async fn resolve_av_url(app: &WebAppState, uuid: &str) -> Result<String, String> {
    let st = app.shared.read().await;
    st.devices
        .iter()
        .find(|d| d.uuid == uuid)
        .map(|d| d.av_transport_url.clone())
        .ok_or_else(|| format!("Device not found: {}", uuid))
}

/// Resolve the media directory: next to the binary or the project `media/` folder.
fn resolve_media_dir() -> PathBuf {
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

// ─── Entry point ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    env_logger::init();

    let media_dir = resolve_media_dir();

    let local_ip = media_server::local_ip().unwrap_or_else(|| "127.0.0.1".to_string());
    let media_base_url = format!("http://{}:8080/media", local_ip);

    let shared = state::new_shared_state();
    {
        let mut s = shared.write().await;
        s.media_server_base_url = media_base_url;
    }

    // Background discovery loop
    let shared_for_bg = shared.clone();
    tokio::spawn(async move {
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

    let app_state = WebAppState {
        shared,
        client: Arc::new(Mutex::new(Client::new())),
        media_dir: media_dir.clone(),
    };

    // CORS — allow the Vue dev server and any other origin
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/devices", get(get_devices))
        .route("/api/devices/discover", post(discover_devices))
        .route("/api/devices/:uuid/play", post(play_on_device))
        .route("/api/devices/:uuid/pause", post(pause_device))
        .route("/api/devices/:uuid/stop", post(stop_device))
        .route("/api/media", get(list_media).post(upload_media))
        .route("/api/scenes", get(get_scenes).post(save_scene))
        .route("/api/scenes/:name", delete(delete_scene))
        .route("/api/scenes/:name/apply", post(apply_scene))
        .route("/api/config/media-server-url", get(get_media_server_url))
        .nest_service("/media", ServeDir::new(media_dir.clone()))
        .route("/web/assets/*path", get(serve_assets))
        .route("/web/favicon.ico", get(serve_favicon))
        .route("/web", get(serve_frontend))
        .route("/web/", get(serve_frontend))
        .fallback(get(serve_frontend))
        .layer(cors)
        .with_state(app_state);

    let (listener, port) = bind_with_fallback(8080).await;

    log::info!("ScreenPilot API server listening on http://0.0.0.0:{}", port);
    log::info!("Web UI: http://localhost:{}/web", port);

    axum::serve(listener, app)
        .await
        .expect("API server error");
}

async fn bind_with_fallback(port: u16) -> (tokio::net::TcpListener, u16) {
    let ports_to_try = (port..=port + 10).collect::<Vec<_>>();
    
    for p in &ports_to_try {
        let addr = format!("0.0.0.0:{}", p);
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                if *p != port {
                    eprintln!("Port {} is in use, using port {} instead", port, p);
                }
                return (listener, *p);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                continue;
            }
            Err(e) => {
                panic!("Failed to bind on {}: {}", p, e);
            }
        }
    }
    
    panic!("Could not bind to any port in range {:?}", ports_to_try);
}
