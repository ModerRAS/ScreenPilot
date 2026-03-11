mod discovery;
mod dlna;
mod frontend;
mod media_server;
mod state;
mod persistence;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::sync::mpsc as tokio_mpsc;
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

    // Persist devices to file
    if let Err(e) = persistence::save_devices(&merged) {
        log::warn!("Failed to save devices: {}", e);
    }

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
        let uri = format!("{}/api/media/stream/{}", st.media_server_base_url, body.media_filename);
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

#[derive(Debug, Clone)]
enum HardwareEncoder {
    None,
    Nvidia,
    IntelQsv,
    AmdVce,
    AppleVtb,
    VAAPI,
}

fn detect_hardware_encoder() -> HardwareEncoder {
    let output = std::process::Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-encoders")
        .output();
    
    match output {
        Ok(o) => {
            let encoders = String::from_utf8_lossy(&o.stdout);
            
            if encoders.contains("h264_amf") {
                log::info!("Using AMD GPU hardware encoding (VCE)");
                return HardwareEncoder::AmdVce;
            }
            
            if encoders.contains("h264_nvenc") {
                log::info!("Using NVIDIA GPU hardware encoding");
                return HardwareEncoder::Nvidia;
            }
            
            if encoders.contains("h264_qsv") {
                log::info!("Using Intel Quick Sync Video hardware encoding");
                return HardwareEncoder::IntelQsv;
            }
            
            if encoders.contains("h264_videotoolbox") {
                log::info!("Using Apple VideoToolbox hardware encoding");
                return HardwareEncoder::AppleVtb;
            }
            
            #[cfg(target_os = "linux")]
            if encoders.contains("h264_vaapi") {
                log::info!("Using VAAPI hardware encoding");
                return HardwareEncoder::VAAPI;
            }
        }
        Err(e) => {
            log::warn!("Failed to detect hardware encoders: {}", e);
        }
    }
    
    log::info!("No hardware encoder found, using software encoding");
    HardwareEncoder::None
}

fn get_encoder_from_preference(pref: &str) -> HardwareEncoder {
    match pref {
        "nvidia" => HardwareEncoder::Nvidia,
        "amd" => HardwareEncoder::AmdVce,
        "intel" => HardwareEncoder::IntelQsv,
        "apple" => HardwareEncoder::AppleVtb,
        "software" => HardwareEncoder::None,
        _ => detect_hardware_encoder(),
    }
}

fn build_encoder_args(hw: &HardwareEncoder) -> (Vec<&'static str>, Vec<&'static str>) {
    match hw {
        HardwareEncoder::Nvidia => (
            vec![
                "-c:v", "h264_nvenc",
                "-preset", "p4",
                "-tune", "ll",
                "-rc", "constqp",
                "-qp", "18",
                "-bf", "3",
            ],
            vec!["-c:a", "aac", "-b:a", "256k"],
        ),
        HardwareEncoder::IntelQsv => (
            vec![
                "-c:v", "h264_qsv",
                "-preset", "veryfast",
                "-global_quality", "18",
            ],
            vec!["-c:a", "aac", "-b:a", "256k"],
        ),
        HardwareEncoder::AmdVce => (
            vec![
                "-c:v", "h264_amf",
                "-preset", "quality",
                "-qp", "18",
            ],
            vec!["-c:a", "aac", "-b:a", "256k"],
        ),
        HardwareEncoder::AppleVtb => (
            vec![
                "-c:v", "h264_videotoolbox",
                "-profile:v", "high",
                "-q", "18",
            ],
            vec!["-c:a", "aac", "-b:a", "256k"],
        ),
        HardwareEncoder::VAAPI => (
            vec![
                "-vaapi_device", "/dev/dri/renderD128",
                "-vf", "format=nv12,hwupload",
                "-c:v", "h264_vaapi",
                "-qp", "18",
            ],
            vec!["-c:a", "aac", "-b:a", "256k"],
        ),
        HardwareEncoder::None => (
            vec![
                "-c:v", "libx264",
                "-preset", "ultrafast",
                "-tune", "zerolatency",
                "-crf", "18",
            ],
            vec!["-c:a", "aac", "-b:a", "256k"],
        ),
    }
}

struct FfmpegProcess {
    child: std::process::Child,
}

impl Drop for FfmpegProcess {
    fn drop(&mut self) {
        if let Err(e) = self.child.kill() {
            log::warn!("Failed to kill ffmpeg: {}", e);
        }
    }
}

use tokio::time::timeout;


/// Helper function to spawn ffmpeg stream with a specific encoder.
/// Returns Ok if stream starts successfully, Err with error message if it fails.
async fn spawn_ffmpeg_stream(
    media_dir: &PathBuf,
    filename: &str,
    encoder: &HardwareEncoder,
) -> Result<(tokio::sync::mpsc::UnboundedReceiver<Result<Vec<u8>, std::io::Error>>, std::thread::JoinHandle<()>), String> {
    use std::process::{Command, Stdio};
    use std::io::Read;
    
    let media_path = media_dir.join(filename);
    if !media_path.exists() {
        return Err("Media file not found".to_string());
    }
    
    let media_path_str = media_path.to_str().unwrap().to_string();
    
    let (video_args, audio_args) = build_encoder_args(encoder);
    
    log::info!("Starting stream with encoder: {:?}", encoder);
    
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-stream_loop").arg("-1");
    cmd.arg("-re");
    cmd.arg("-i").arg(&media_path_str);
    
    for arg in video_args {
        cmd.arg(arg);
    }
    for arg in audio_args {
        cmd.arg(arg);
    }
    
    cmd.arg("-f").arg("mpegts");
    cmd.arg("-");
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    
    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to start ffmpeg: {}", e))?;

    let mut stdout = child.stdout.take().ok_or_else(|| 
        "Failed to capture ffmpeg output".to_string())?;

    let mut ffmpeg = FfmpegProcess { child };
    
    let encoder_for_check = encoder.clone();
    let stderr_thread = std::thread::spawn(move || {
        use std::io::Read;
        let mut error_output = String::new();
        if let Some(stderr) = ffmpeg.child.stderr.take() {
            let mut stderr = stderr;
            let mut buf = [0u8; 4096];
            loop {
                match stderr.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let msg = String::from_utf8_lossy(&buf[..n]);
                        error_output.push_str(&msg);
                        if msg.to_lowercase().contains("error") 
                            || msg.to_lowercase().contains("failed")
                            || msg.to_lowercase().contains("cannot")
                            || msg.to_lowercase().contains("invalid") {
                            log::error!("ffmpeg: {}", msg);
                        } else {
                            log::warn!("ffmpeg: {}", msg);
                        }
                    }
                    Err(_) => break,
                }
            }
        }
        // Check for hardware encoder specific failures
        if !matches!(encoder_for_check, HardwareEncoder::None) {
            let lower = error_output.to_lowercase();
            if lower.contains("cannot load") 
                || lower.contains("failed to open")
                || lower.contains("not found")
                || lower.contains("no device") {
                log::warn!("Hardware encoder {:?} failed to initialize: {}", encoder_for_check, error_output);
            }
        }
    });
    
    let (tx, rx) = tokio_mpsc::unbounded_channel::<Result<Vec<u8>, std::io::Error>>();
    
    let _stdout_thread = std::thread::spawn(move || {
        let mut buf = [0u8; 65536];
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(Ok(buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    
    Ok((rx, stderr_thread))
}


async fn stream_media(
    State(app): State<WebAppState>,
    Path(filename): Path<String>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    validate_media_filename(&filename)?;
    
    let media_path = app.media_dir.join(&filename);
    if !media_path.exists() {
        return Err(error_response(StatusCode::NOT_FOUND, "Media file not found"));
    }
    
    let encoder_pref = {
        let st = app.shared.read().await;
        st.preferred_encoder.clone()
    };
    
    let hw_encoder = get_encoder_from_preference(&encoder_pref);
    log::info!("Using encoder preference: {} -> {:?}", encoder_pref, hw_encoder);
    
    // First try with hardware encoder (if available)
    if !matches!(hw_encoder, HardwareEncoder::None) {
        match timeout(Duration::from_secs(5), spawn_ffmpeg_stream(&app.media_dir, &filename, &hw_encoder)).await {
            Ok(Ok((mut rx, _stderr_thread))) => {
                // Hardware encoder started, verify it actually produces output
                match timeout(Duration::from_secs(3), async { rx.recv().await }).await {
                    Ok(Some(Ok(_chunk))) => {
                        log::debug!("Hardware encoder {:?} started successfully", hw_encoder);
                    }
                    Ok(Some(Err(e))) => {
                        log::warn!("Hardware encoder {:?} produced error: {}. Falling back to software.", hw_encoder, e);
                        // Fall through to software encoder
                    }
                    Ok(None) => {
                        log::warn!("Hardware encoder {:?} stream ended unexpectedly. Falling back to software.", hw_encoder);
                        // Fall through to software encoder
                    }
                    Err(_) => {
                        log::warn!("Hardware encoder {:?} may have failed to initialize (no output within 3s). Falling back to software.", hw_encoder);
                        // Fall through to software encoder
                    }
                }
                
                // If we got here with a working stream, use it
                // Otherwise fall through to software encoder
                if matches!(timeout(Duration::from_secs(1), async { rx.recv().await }).await, Ok(Some(Ok(_)))) {
                    let stream = async_stream::stream! {
                        while let Some(chunk) = rx.recv().await {
                            yield chunk;
                        }
                    };
                    
                    return Ok((
                        [("Content-Type", "video/mp2t")],
                        axum::body::Body::from_stream(stream)
                    ));
                }
            }
            Ok(Err(e)) => {
                log::warn!("Hardware encoder {:?} failed to start: {}. Falling back to software.", hw_encoder, e);
                // Fall through to software encoder
            }
            Err(_) => {
                log::warn!("Hardware encoder {:?} timed out during initialization. Falling back to software.", hw_encoder);
                // Fall through to software encoder
            }
        }
    }
    
    // Fall back to software encoder
    let software_encoder = HardwareEncoder::None;
    log::info!("Using software encoder (libx264)");
    
    let (mut rx, _stderr_thread) = spawn_ffmpeg_stream(&app.media_dir, &filename, &software_encoder)
        .await
        .map_err(|e| error_response(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    
    let stream = async_stream::stream! {
        while let Some(chunk) = rx.recv().await {
            yield chunk;
        }
    };
    
    Ok((
        [("Content-Type", "video/mp2t")],
        axum::body::Body::from_stream(stream)
    ))
}

/// POST /api/media/upload
async fn upload_media(
    State(app): State<WebAppState>,
    mut multipart: Multipart,
) -> Result<StatusCode, ApiError> {
    const ALLOWED_EXTENSIONS: &[&str] = &["mp4", "webm", "avi", "mkv", "mov"];

    while let Some(mut field) = multipart.next_field().await.map_err(|e| {
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

            let dest_path = app.media_dir.join(&filename);
            
            use tokio::io::AsyncWriteExt;
            let mut file = tokio::fs::File::create(&dest_path).await.map_err(|e| {
                error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create file: {}", e))
            })?;

            while let Some(chunk) = field.chunk().await.map_err(|e| {
                error_response(StatusCode::BAD_REQUEST, format!("Failed to read file: {}", e))
            })? {
                file.write_all(&chunk).await.map_err(|e| {
                    error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write file: {}", e))
                })?;
            }

            file.flush().await.map_err(|e| {
                error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to flush file: {}", e))
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
        let media_uri = format!("{}/api/media/stream/{}", media_base, filename);
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

/// GET /api/config/encoder — get current encoder preference.
async fn get_encoder(State(app): State<WebAppState>) -> Json<String> {
    let st = app.shared.read().await;
    Json(st.preferred_encoder.clone())
}

/// PUT /api/config/encoder — set encoder preference.
async fn set_encoder(
    State(app): State<WebAppState>,
    Json(body): Json<SetEncoderRequest>,
) -> Result<StatusCode, ApiError> {
    let valid = ["auto", "nvidia", "amd", "intel", "apple", "software"];
    if !valid.contains(&body.encoder.as_str()) {
        return Err(error_response(StatusCode::BAD_REQUEST, "Invalid encoder"));
    }
    let mut st = app.shared.write().await;
    st.preferred_encoder = body.encoder;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct SetEncoderRequest {
    encoder: String,
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
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let media_dir = resolve_media_dir();

    let local_ip = media_server::local_ip().unwrap_or_else(|| "127.0.0.1".to_string());
    let media_base_url = format!("http://{}:8080", local_ip);

    let shared = state::new_shared_state();

    // Load saved devices
    {
        let mut s = shared.write().await;
        s.devices = persistence::load_devices();
        log::info!("Loaded {} devices from persistence", s.devices.len());
        s.media_server_base_url = media_base_url;
        s.preferred_encoder = "auto".to_string();
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
            let devices_to_save = st.devices.clone();
            drop(st);

            // Save devices after background discovery
            if let Err(e) = persistence::save_devices(&devices_to_save) {
                log::warn!("Failed to save devices in background: {}", e);
            }

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
        .route("/api/media", get(list_media))
        .route("/api/media/upload", post(upload_media).layer(DefaultBodyLimit::max(100 * 1024 * 1024 * 1024)))
        .route("/api/media/stream/*path", get(stream_media))
        .route("/api/scenes", get(get_scenes).post(save_scene))
        .route("/api/scenes/:name", delete(delete_scene))
        .route("/api/scenes/:name/apply", post(apply_scene))
        .route("/api/config/media-server-url", get(get_media_server_url))
        .route("/api/config/encoder", get(get_encoder).put(set_encoder))
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
