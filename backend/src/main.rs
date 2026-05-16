mod config;
mod discovery;
mod dlna;
pub mod encoder;
mod frontend;
mod media_server;
mod persistence;
mod state;

use crate::encoder::{detect_hw_encoders, DetectionResult};

use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path as FilePath, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, UNIX_EPOCH};

use axum::extract::{DefaultBodyLimit, Multipart, Path, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc as tokio_mpsc;
use tokio::sync::Mutex;
use tokio_util::io::ReaderStream;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

use frontend::Frontend;
use state::{PlaybackStatus, RendererDevice, Scene, SharedState};

static CACHE_JOB_LOCKS: Lazy<StdMutex<HashMap<PathBuf, Arc<Mutex<()>>>>> =
    Lazy::new(|| StdMutex::new(HashMap::new()));

const AUTH_COOKIE_NAME: &str = "screenpilot_session";
const AUTH_COOKIE_MAX_AGE_SECONDS: u64 = 7 * 24 * 60 * 60;
const ALLOWED_MEDIA_EXTENSIONS: &[&str] = &["mp4", "webm", "avi", "mkv", "mov"];
const MAX_UPLOAD_BYTES: usize = 256usize * 1024 * 1024 * 1024;

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
    pub cached_encoders: Arc<Mutex<Option<DetectionResult>>>,
    pub auth: Arc<AuthState>,
}

#[derive(Debug)]
pub struct AuthState {
    username: String,
    password: String,
    active_sessions: StdMutex<HashSet<String>>,
}

impl AuthState {
    fn from_config(config: config::AuthSettings) -> Self {
        Self {
            username: config.username,
            password: config.password,
            active_sessions: StdMutex::new(HashSet::new()),
        }
    }

    fn username(&self) -> &str {
        &self.username
    }

    fn verify_credentials(&self, username: &str, password: &str) -> bool {
        constant_time_eq(username, &self.username) && constant_time_eq(password, &self.password)
    }

    fn issue_session_token(&self) -> String {
        let token = generate_secret();
        self.active_sessions
            .lock()
            .expect("active session lock poisoned")
            .insert(token.clone());
        token
    }

    fn revoke_session_token(&self, token: &str) {
        self.active_sessions
            .lock()
            .expect("active session lock poisoned")
            .retain(|active| !constant_time_eq(active, token));
    }

    fn verify_session_token(&self, token: &str) -> bool {
        self.active_sessions
            .lock()
            .expect("active session lock poisoned")
            .iter()
            .any(|active| constant_time_eq(active, token))
    }
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
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct AuthStatusResponse {
    authenticated: bool,
    username: Option<String>,
}

#[derive(Deserialize)]
struct SetDeviceAliasRequest {
    alias: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct MediaFileInfo {
    name: String,
    size: u64,
    modified: Option<u64>,
    extension: String,
}

#[derive(Serialize)]
struct UploadMediaResponse {
    file: MediaFileInfo,
}

#[derive(Deserialize)]
struct RenameMediaRequest {
    new_name: String,
}

#[derive(Deserialize)]
struct SaveSceneRequest {
    name: String,
    assignments: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

type ApiError = (StatusCode, Json<ErrorResponse>);

const MAX_DEVICE_ALIAS_CHARS: usize = 64;

fn error_response(status: StatusCode, msg: impl Into<String>) -> ApiError {
    (status, Json(ErrorResponse { error: msg.into() }))
}

fn generate_secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let max_len = left.len().max(right.len());
    let mut diff = left.len() ^ right.len();

    for index in 0..max_len {
        let left_byte = left.get(index).copied().unwrap_or(0);
        let right_byte = right.get(index).copied().unwrap_or(0);
        diff |= (left_byte ^ right_byte) as usize;
    }

    diff == 0
}

fn session_cookie(token: &str) -> String {
    format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        AUTH_COOKIE_NAME, token, AUTH_COOKIE_MAX_AGE_SECONDS
    )
}

fn expired_session_cookie() -> String {
    format!(
        "{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0",
        AUTH_COOKIE_NAME
    )
}

fn session_token_from_headers(headers: &HeaderMap) -> Option<&str> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;

    cookie.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        (name == AUTH_COOKIE_NAME).then_some(value)
    })
}

fn headers_have_valid_session(auth: &AuthState, headers: &HeaderMap) -> bool {
    session_token_from_headers(headers).is_some_and(|token| auth.verify_session_token(token))
}

fn is_public_without_auth(path: &str) -> bool {
    !path.starts_with("/api/")
        || path.starts_with("/api/auth/")
        || path.starts_with("/api/media/stream/")
}

async fn require_auth(State(app): State<WebAppState>, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    if is_public_without_auth(path) || headers_have_valid_session(&app.auth, request.headers()) {
        return next.run(request).await;
    }

    error_response(StatusCode::UNAUTHORIZED, "Authentication required").into_response()
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

fn validate_media_extension(filename: &str) -> Result<String, ApiError> {
    let ext = filename
        .rsplit('.')
        .next()
        .map(|e| e.to_lowercase())
        .ok_or_else(|| error_response(StatusCode::BAD_REQUEST, "No file extension"))?;

    if ext == filename.to_lowercase() || !ALLOWED_MEDIA_EXTENSIONS.contains(&ext.as_str()) {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid file extension '{}'. Allowed: {:?}",
                ext, ALLOWED_MEDIA_EXTENSIONS
            ),
        ));
    }

    Ok(ext)
}

fn validate_media_upload_filename(filename: &str) -> Result<String, ApiError> {
    validate_media_filename(filename)?;
    if filename.starts_with('.') {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "Media filename must not start with a dot",
        ));
    }
    validate_media_extension(filename)
}

fn normalize_device_alias(alias: Option<String>) -> Result<Option<String>, ApiError> {
    let Some(alias) = alias else {
        return Ok(None);
    };

    if alias.contains('\0') {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "Invalid device alias",
        ));
    }

    let alias = alias.trim();
    if alias.is_empty() {
        return Ok(None);
    }

    if alias.chars().count() > MAX_DEVICE_ALIAS_CHARS {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            format!(
                "Device alias must be {} characters or fewer",
                MAX_DEVICE_ALIAS_CHARS
            ),
        ));
    }

    Ok(Some(alias.to_string()))
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

async fn login(
    State(app): State<WebAppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    if !app.auth.verify_credentials(&body.username, &body.password) {
        return Err(error_response(
            StatusCode::UNAUTHORIZED,
            "Invalid username or password",
        ));
    }

    let mut response = Json(AuthStatusResponse {
        authenticated: true,
        username: Some(app.auth.username().to_string()),
    })
    .into_response();

    let token = app.auth.issue_session_token();
    let cookie = HeaderValue::from_str(&session_cookie(&token)).map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create session cookie",
        )
    })?;
    response.headers_mut().insert(header::SET_COOKIE, cookie);

    Ok(response)
}

async fn logout(State(app): State<WebAppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    if let Some(token) = session_token_from_headers(&headers) {
        app.auth.revoke_session_token(token);
    }

    let mut response = Json(AuthStatusResponse {
        authenticated: false,
        username: None,
    })
    .into_response();

    let cookie = HeaderValue::from_str(&expired_session_cookie()).map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to clear session cookie",
        )
    })?;
    response.headers_mut().insert(header::SET_COOKIE, cookie);

    Ok(response)
}

async fn auth_status(
    State(app): State<WebAppState>,
    headers: HeaderMap,
) -> Json<AuthStatusResponse> {
    let authenticated = headers_have_valid_session(&app.auth, &headers);
    Json(AuthStatusResponse {
        authenticated,
        username: authenticated.then(|| app.auth.username().to_string()),
    })
}

/// GET /api/devices — return the current device list without triggering a new scan.
async fn get_devices(State(app): State<WebAppState>) -> Json<Vec<RendererDevice>> {
    let st = app.shared.read().await;
    Json(st.devices.clone())
}

/// POST /api/devices/discover — trigger SSDP discovery and return updated list.
async fn discover_devices(State(app): State<WebAppState>) -> Json<Vec<RendererDevice>> {
    let devices = discovery::discover_renderers().await;
    let mut st = app.shared.write().await;

    let existing: HashMap<String, RendererDevice> = st
        .devices
        .iter()
        .map(|d| (d.uuid.clone(), d.clone()))
        .collect();

    let mut merged: Vec<RendererDevice> = devices
        .into_iter()
        .map(|mut d| {
            if let Some(old) = existing.get(&d.uuid) {
                d.alias = old.alias.clone();
                d.status = old.status.clone();
                d.current_media = old.current_media.clone();
                d.loop_playback = old.loop_playback;
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

/// PUT /api/devices/:uuid/alias — set or clear a device alias.
async fn set_device_alias(
    State(app): State<WebAppState>,
    Path(device_uuid): Path<String>,
    Json(body): Json<SetDeviceAliasRequest>,
) -> Result<Json<RendererDevice>, ApiError> {
    let alias = normalize_device_alias(body.alias)?;

    let (updated, devices_to_save) = {
        let mut st = app.shared.write().await;
        let device = st
            .devices
            .iter_mut()
            .find(|d| d.uuid == device_uuid)
            .ok_or_else(|| {
                error_response(
                    StatusCode::NOT_FOUND,
                    format!("Device not found: {}", device_uuid),
                )
            })?;

        device.alias = alias;
        (device.clone(), st.devices.clone())
    };

    persistence::save_devices(&devices_to_save).map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save device alias: {}", e),
        )
    })?;

    Ok(Json(updated))
}

/// POST /api/devices/:uuid/play — play a media file on a specific device.
async fn play_on_device(
    State(app): State<WebAppState>,
    Path(device_uuid): Path<String>,
    Json(body): Json<PlayRequest>,
) -> Result<StatusCode, ApiError> {
    validate_media_filename(&body.media_filename)?;

    let (av_url, media_base) = {
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
        (
            device.av_transport_url.clone(),
            st.media_server_base_url.clone(),
        )
    };
    let media_uri = prepare_media_uri(&app, &media_base, &body.media_filename).await?;

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
    let av_url = resolve_av_url(&app, &device_uuid)
        .await
        .map_err(|e| error_response(StatusCode::NOT_FOUND, e))?;
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
    let av_url = resolve_av_url(&app, &device_uuid)
        .await
        .map_err(|e| error_response(StatusCode::NOT_FOUND, e))?;
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

async fn get_device_loop(
    State(app): State<WebAppState>,
    Path(device_uuid): Path<String>,
) -> Result<Json<bool>, ApiError> {
    let st = app.shared.read().await;
    let device = st
        .devices
        .iter()
        .find(|d| d.uuid == device_uuid)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "Device not found"))?;
    Ok(Json(device.loop_playback))
}

async fn set_device_loop(
    State(app): State<WebAppState>,
    Path(device_uuid): Path<String>,
    Json(body): Json<SetLoopRequest>,
) -> Result<StatusCode, ApiError> {
    let mut st = app.shared.write().await;
    if let Some(device) = st.devices.iter_mut().find(|d| d.uuid == device_uuid) {
        device.loop_playback = body.loop_playback;
        log::info!("Device {} loop set to {}", device_uuid, body.loop_playback);
    }
    Ok(StatusCode::OK)
}

fn modified_unix_seconds(metadata: &std::fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

fn media_file_info(media_dir: &PathBuf, filename: &str) -> Result<MediaFileInfo, ApiError> {
    validate_media_upload_filename(filename)?;

    let path = media_dir.join(filename);
    let metadata = std::fs::metadata(&path).map_err(|e| {
        error_response(
            StatusCode::NOT_FOUND,
            format!("Media file not found: {}", e),
        )
    })?;

    if !metadata.is_file() {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "Media file not found",
        ));
    }

    Ok(MediaFileInfo {
        name: filename.to_string(),
        size: metadata.len(),
        modified: modified_unix_seconds(&metadata),
        extension: validate_media_extension(filename)?,
    })
}

fn list_media_file_infos(media_dir: &PathBuf) -> Vec<MediaFileInfo> {
    media_server::list_media_files(media_dir)
        .into_iter()
        .filter(|filename| validate_media_upload_filename(filename).is_ok())
        .filter_map(|filename| media_file_info(media_dir, &filename).ok())
        .collect()
}

/// GET /api/media — list available media filenames.
async fn list_media(State(app): State<WebAppState>) -> Json<Vec<String>> {
    Json(
        list_media_file_infos(&app.media_dir)
            .into_iter()
            .map(|file| file.name)
            .collect(),
    )
}

/// GET /api/media/files — list media files with metadata.
async fn list_media_files(State(app): State<WebAppState>) -> Json<Vec<MediaFileInfo>> {
    Json(list_media_file_infos(&app.media_dir))
}

#[derive(Debug, Clone)]
enum HardwareEncoder {
    None,
    Nvidia,
    IntelQsv,
    AmdVce,
    AppleVtb,
    Vaapi,
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
                return HardwareEncoder::Vaapi;
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
        "vaapi" => HardwareEncoder::Vaapi,
        "software" => HardwareEncoder::None,
        _ => detect_hardware_encoder(),
    }
}

fn build_encoder_args(hw: &HardwareEncoder) -> (Vec<&'static str>, Vec<&'static str>) {
    match hw {
        HardwareEncoder::Nvidia => (
            vec![
                "-c:v",
                "h264_nvenc",
                "-preset",
                "p4",
                "-tune",
                "ll",
                "-rc",
                "constqp",
                "-qp",
                "18",
                "-bf",
                "0",
                "-level",
                "4.0",
            ],
            vec!["-c:a", "aac", "-b:a", "128k"],
        ),
        HardwareEncoder::IntelQsv => (
            vec![
                "-c:v",
                "h264_qsv",
                "-preset",
                "veryfast",
                "-global_quality",
                "18",
            ],
            vec!["-c:a", "aac", "-b:a", "128k"],
        ),
        HardwareEncoder::AmdVce => (
            vec!["-c:v", "h264_amf", "-preset", "quality", "-qp", "18"],
            vec!["-c:a", "aac", "-b:a", "128k"],
        ),
        HardwareEncoder::AppleVtb => (
            vec!["-c:v", "h264_videotoolbox", "-q", "18"],
            vec!["-c:a", "aac", "-b:a", "128k"],
        ),
        HardwareEncoder::Vaapi => (
            vec![
                "-vaapi_device",
                "/dev/dri/renderD128",
                "-vf",
                "format=nv12,hwupload",
                "-c:v",
                "h264_vaapi",
                "-qp",
                "18",
            ],
            vec!["-c:a", "aac", "-b:a", "128k"],
        ),
        HardwareEncoder::None => (
            vec![
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-tune",
                "zerolatency",
                "-crf",
                "18",
                "-profile:v",
                "baseline",
                "-pix_fmt",
                "yuv420p",
                "-level",
                "4.0",
            ],
            vec!["-c:a", "aac", "-b:a", "128k"],
        ),
    }
}

fn get_cache_dir(media_dir: &PathBuf) -> PathBuf {
    media_dir.join(".cache")
}

#[derive(Debug, Clone, Copy)]
enum CacheProfile {
    RemuxTs,
    TranscodedH264Aac,
}

impl CacheProfile {
    fn suffix(self) -> &'static str {
        match self {
            CacheProfile::RemuxTs => "dlna-remux-ts-v1",
            CacheProfile::TranscodedH264Aac => "dlna-h264-aac-v2",
        }
    }
}

fn get_cache_path(media_dir: &PathBuf, filename: &str, profile: CacheProfile) -> PathBuf {
    let cache_dir = get_cache_dir(media_dir);
    let safe_name = filename.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
    let cached_name = format!("{}.{}.ts", safe_name, profile.suffix());
    cache_dir.join(cached_name)
}

fn check_cache(media_dir: &PathBuf, filename: &str, profile: CacheProfile) -> Option<PathBuf> {
    let cache_path = get_cache_path(media_dir, filename, profile);
    if cache_path.exists() {
        let original_path = media_dir.join(filename);
        if let (Ok(original_meta), Ok(cache_meta)) =
            (original_path.metadata(), cache_path.metadata())
        {
            if let (Ok(original_modified), Ok(cache_modified)) =
                (original_meta.modified(), cache_meta.modified())
            {
                if cache_modified > original_modified {
                    log::info!("Cache found: {:?}", cache_path);
                    return Some(cache_path);
                }
            }
        }
    }
    None
}

fn cache_job_lock(media_dir: &PathBuf, filename: &str, profile: CacheProfile) -> Arc<Mutex<()>> {
    let cache_path = get_cache_path(media_dir, filename, profile);
    let mut locks = CACHE_JOB_LOCKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    Arc::clone(
        locks
            .entry(cache_path)
            .or_insert_with(|| Arc::new(Mutex::new(()))),
    )
}

async fn with_cache_job_lock<F, Fut>(
    media_dir: &PathBuf,
    filename: &str,
    profile: CacheProfile,
    create_cache: F,
) -> Result<PathBuf, String>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<PathBuf, String>>,
{
    if let Some(cache_path) = check_cache(media_dir, filename, profile) {
        return Ok(cache_path);
    }

    let lock = cache_job_lock(media_dir, filename, profile);
    let _guard = lock.lock().await;

    if let Some(cache_path) = check_cache(media_dir, filename, profile) {
        return Ok(cache_path);
    }

    create_cache().await
}

#[derive(Debug, Clone)]
struct MediaInfo {
    format_name: Option<String>,
    video_codec: Option<String>,
    video_profile: Option<String>,
    video_level: Option<u32>,
    video_pix_fmt: Option<String>,
    audio_codec: Option<String>,
    video_width: Option<u32>,
    video_height: Option<u32>,
    audio_channels: Option<u32>,
    audio_sample_rate: Option<u32>,
}

fn probe_media_info(media_path: &PathBuf) -> Option<MediaInfo> {
    let output = std::process::Command::new("ffprobe")
        .arg("-v")
        .arg("quiet")
        .arg("-print_format")
        .arg("json")
        .arg("-show_format")
        .arg("-show_streams")
        .arg(media_path.as_os_str())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&json_str).ok()?;

    let mut info = MediaInfo {
        format_name: json
            .get("format")
            .and_then(|f| f.get("format_name"))
            .and_then(|v| v.as_str())
            .map(String::from),
        video_codec: None,
        video_profile: None,
        video_level: None,
        video_pix_fmt: None,
        audio_codec: None,
        video_width: None,
        video_height: None,
        audio_channels: None,
        audio_sample_rate: None,
    };

    if let Some(streams) = json.get("streams").and_then(|s| s.as_array()) {
        for stream in streams {
            let codec_type = stream.get("codec_type").and_then(|v| v.as_str());
            match codec_type {
                Some("video") if info.video_codec.is_none() => {
                    info.video_codec = stream
                        .get("codec_name")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    info.video_profile = stream
                        .get("profile")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    info.video_level = stream
                        .get("level")
                        .and_then(|v| v.as_u64())
                        .map(|level| level as u32);
                    info.video_pix_fmt = stream
                        .get("pix_fmt")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    info.video_width = stream
                        .get("width")
                        .and_then(|v| v.as_u64())
                        .map(|w| w as u32);
                    info.video_height = stream
                        .get("height")
                        .and_then(|v| v.as_u64())
                        .map(|h| h as u32);
                }
                Some("audio") if info.audio_codec.is_none() => {
                    info.audio_codec = stream
                        .get("codec_name")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    info.audio_channels = stream
                        .get("channels")
                        .and_then(|v| v.as_u64())
                        .map(|c| c as u32);
                    info.audio_sample_rate = stream
                        .get("sample_rate")
                        .and_then(|v| v.as_str())
                        .and_then(|rate| rate.parse::<u32>().ok());
                }
                _ => {}
            }
        }
    }

    Some(info)
}

#[derive(Debug, Clone)]
enum PreparedMedia {
    Original(PathBuf),
    Remuxed(PathBuf),
    Transcoded(PathBuf),
}

impl PreparedMedia {
    fn path(&self) -> &PathBuf {
        match self {
            PreparedMedia::Original(path)
            | PreparedMedia::Remuxed(path)
            | PreparedMedia::Transcoded(path) => path,
        }
    }

    fn content_type(&self) -> &'static str {
        match self {
            PreparedMedia::Original(path) => content_type_for_path(path),
            PreparedMedia::Remuxed(_) | PreparedMedia::Transcoded(_) => "video/mp2t",
        }
    }

    fn needs_loop_stream(&self, loop_playback: bool) -> bool {
        loop_playback
    }
}

fn is_dlna_bypass_compatible(info: &MediaInfo, filename: &str) -> bool {
    is_safe_dlna_container(info, filename) && has_safe_dlna_codecs(info)
}

fn has_safe_dlna_codecs(info: &MediaInfo) -> bool {
    is_safe_h264_video(info) && is_safe_dlna_audio(info)
}

fn is_safe_dlna_container(info: &MediaInfo, filename: &str) -> bool {
    let ext = filename
        .rsplit('.')
        .next()
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();

    let format_names: Vec<&str> = info
        .format_name
        .as_deref()
        .unwrap_or("")
        .split(',')
        .collect();

    let mp4_family = matches!(ext.as_str(), "mp4" | "m4v" | "mov")
        || format_names
            .iter()
            .any(|name| matches!(*name, "mp4" | "mov" | "m4v"));
    let ts_family = matches!(ext.as_str(), "ts" | "m2ts" | "mts")
        || format_names
            .iter()
            .any(|name| matches!(*name, "mpegts" | "mpegtsraw"));

    mp4_family || ts_family
}

fn is_safe_h264_video(info: &MediaInfo) -> bool {
    if info.video_codec.as_deref() != Some("h264") {
        return false;
    }

    if let Some(profile) = &info.video_profile {
        let profile = profile.to_ascii_lowercase();
        let profile_ok = profile.contains("baseline") || profile == "main" || profile == "high";
        if !profile_ok {
            return false;
        }
    }

    if let Some(level) = info.video_level {
        if level > 41 {
            return false;
        }
    }

    if let Some(pix_fmt) = &info.video_pix_fmt {
        if pix_fmt != "yuv420p" {
            return false;
        }
    }

    if let (Some(width), Some(height)) = (info.video_width, info.video_height) {
        if width > 1920 || height > 1080 {
            return false;
        }
    }

    true
}

fn is_safe_dlna_audio(info: &MediaInfo) -> bool {
    let Some(codec) = info.audio_codec.as_deref() else {
        return true;
    };

    if !matches!(codec, "aac" | "mp3") {
        return false;
    }

    if let Some(channels) = info.audio_channels {
        if channels > 2 {
            return false;
        }
    }

    if let Some(sample_rate) = info.audio_sample_rate {
        if !matches!(sample_rate, 44_100 | 48_000) {
            return false;
        }
    }

    true
}

async fn transcode_to_cache(
    media_dir: &PathBuf,
    filename: &str,
    encoder: &HardwareEncoder,
) -> Result<PathBuf, String> {
    let media_dir = media_dir.clone();
    let filename = filename.to_string();
    let encoder = encoder.clone();

    tokio::task::spawn_blocking(move || transcode_to_cache_blocking(media_dir, filename, encoder))
        .await
        .map_err(|e| format!("Transcoding task failed: {}", e))?
}

async fn remux_to_cache(media_dir: &PathBuf, filename: &str) -> Result<PathBuf, String> {
    let media_dir = media_dir.clone();
    let filename = filename.to_string();

    tokio::task::spawn_blocking(move || remux_to_cache_blocking(media_dir, filename))
        .await
        .map_err(|e| format!("Remux task failed: {}", e))?
}

async fn remux_to_cache_once(media_dir: &PathBuf, filename: &str) -> Result<PathBuf, String> {
    with_cache_job_lock(media_dir, filename, CacheProfile::RemuxTs, || async {
        remux_to_cache(media_dir, filename).await
    })
    .await
}

fn remux_to_cache_blocking(media_dir: PathBuf, filename: String) -> Result<PathBuf, String> {
    let cache_path = get_cache_path(&media_dir, &filename, CacheProfile::RemuxTs);
    run_ffmpeg_cache_job(
        media_dir,
        filename,
        cache_path,
        |cmd| {
            cmd.arg("-c").arg("copy");
        },
        "Remux",
    )
}

fn transcode_to_cache_blocking(
    media_dir: PathBuf,
    filename: String,
    encoder: HardwareEncoder,
) -> Result<PathBuf, String> {
    let cache_path = get_cache_path(&media_dir, &filename, CacheProfile::TranscodedH264Aac);
    let (video_args, audio_args) = build_encoder_args(&encoder);
    log::info!("Transcoding DLNA-safe cache with encoder: {:?}", encoder);

    run_ffmpeg_cache_job(
        media_dir,
        filename,
        cache_path,
        |cmd| {
            for arg in video_args {
                cmd.arg(arg);
            }
            for arg in audio_args {
                cmd.arg(arg);
            }
        },
        "Transcode",
    )
}

fn should_fallback_to_software(encoder: &HardwareEncoder) -> bool {
    !matches!(encoder, HardwareEncoder::None)
}

fn software_fallback_failure_message(
    encoder: &HardwareEncoder,
    hardware_error: &str,
    software_error: &str,
) -> String {
    format!(
        "Preferred encoder {:?} failed: {}; software fallback failed: {}",
        encoder, hardware_error, software_error
    )
}

async fn transcode_to_cache_once(
    media_dir: &PathBuf,
    filename: &str,
    encoder: &HardwareEncoder,
) -> Result<PathBuf, String> {
    with_cache_job_lock(
        media_dir,
        filename,
        CacheProfile::TranscodedH264Aac,
        || async {
            match transcode_to_cache(media_dir, filename, encoder).await {
                Ok(path) => Ok(path),
                Err(hardware_error) if should_fallback_to_software(encoder) => {
                    log::warn!(
                        "Preferred encoder {:?} failed, falling back to software: {}",
                        encoder,
                        hardware_error
                    );
                    transcode_to_cache(media_dir, filename, &HardwareEncoder::None)
                        .await
                        .map_err(|software_error| {
                            software_fallback_failure_message(
                                encoder,
                                &hardware_error,
                                &software_error,
                            )
                        })
                }
                Err(e) => Err(e),
            }
        },
    )
    .await
}

fn run_ffmpeg_cache_job(
    media_dir: PathBuf,
    filename: String,
    cache_path: PathBuf,
    add_codec_args: impl FnOnce(&mut std::process::Command),
    label: &str,
) -> Result<PathBuf, String> {
    let cache_dir = get_cache_dir(&media_dir);

    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("Failed to create cache dir: {}", e))?;

    let media_path = media_dir.join(&filename);
    let media_path_str = media_path
        .to_str()
        .ok_or_else(|| "Media path is not valid UTF-8".to_string())?
        .to_string();
    let tmp_path = cache_path.with_extension("ts.tmp");
    let tmp_path_str = tmp_path
        .to_str()
        .ok_or_else(|| "Cache path is not valid UTF-8".to_string())?
        .to_string();

    let _ = std::fs::remove_file(&tmp_path);

    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.arg("-y")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-nostdin")
        .arg("-i")
        .arg(&media_path_str);

    cmd.arg("-map").arg("0:v:0").arg("-map").arg("0:a:0?");

    add_codec_args(&mut cmd);

    cmd.arg("-f")
        .arg("mpegts")
        .arg("-mpegts_flags")
        .arg("+resend_headers")
        .arg("-muxdelay")
        .arg("0")
        .arg("-muxpreload")
        .arg("0")
        .arg(&tmp_path_str);

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to start ffmpeg for caching: {}", e))?;

    if output.status.success() {
        let _ = std::fs::remove_file(&cache_path);
        std::fs::rename(&tmp_path, &cache_path)
            .map_err(|e| format!("Failed to move cache into place: {}", e))?;
        log::info!("{} cache created successfully: {:?}", label, cache_path);
        return Ok(cache_path);
    }

    let _ = std::fs::remove_file(&tmp_path);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!("{} failed: {}", label, stderr.trim()))
}

async fn prepare_media_for_dlna(
    media_dir: &PathBuf,
    filename: &str,
    encoder: &HardwareEncoder,
) -> Result<PreparedMedia, String> {
    let media_path = media_dir.join(filename);
    if !media_path.exists() {
        return Err("Media file not found".to_string());
    }

    if let Some(info) = probe_media_info(&media_path) {
        if is_dlna_bypass_compatible(&info, filename) {
            log::info!("Bypassing transcode for DLNA-safe media: {}", filename);
            return Ok(PreparedMedia::Original(media_path));
        }
        if has_safe_dlna_codecs(&info) {
            log::info!("Remuxing safe codecs into DLNA-friendly TS: {}", filename);
            return remux_to_cache_once(media_dir, filename)
                .await
                .map(PreparedMedia::Remuxed);
        }
        log::info!("Media requires DLNA-safe transcode: {:?}", info);
    } else {
        log::warn!(
            "Could not probe media info, transcoding conservatively: {}",
            filename
        );
    }

    transcode_to_cache_once(media_dir, filename, encoder)
        .await
        .map(PreparedMedia::Transcoded)
}

fn media_stream_url(media_base: &str, filename: &str) -> String {
    format!(
        "{}/api/media/stream/{}",
        media_base.trim_end_matches('/'),
        percent_encode_path_segment(filename)
    )
}

fn percent_encode_path_segment(segment: &str) -> String {
    let mut encoded = String::new();
    for byte in segment.bytes() {
        let keep = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        if keep {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{:02X}", byte));
        }
    }
    encoded
}

async fn prepare_media_uri(
    app: &WebAppState,
    media_base: &str,
    filename: &str,
) -> Result<String, ApiError> {
    let encoder_pref = {
        let st = app.shared.read().await;
        st.preferred_encoder.clone()
    };
    let encoder = get_encoder_from_preference(&encoder_pref);

    prepare_media_for_dlna(&app.media_dir, filename, &encoder)
        .await
        .map_err(|e| error_response(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(media_stream_url(media_base, filename))
}

async fn serve_file_response(
    path: &FilePath,
    content_type: &'static str,
) -> Result<axum::response::Response, ApiError> {
    let file = tokio::fs::File::open(path).await.map_err(|e| {
        error_response(
            StatusCode::NOT_FOUND,
            format!("Media file not found: {}", e),
        )
    })?;
    let stream = ReaderStream::new(file);

    Ok((
        [("Content-Type", content_type)],
        axum::body::Body::from_stream(stream),
    )
        .into_response())
}

fn stream_copy_with_ffmpeg(
    path: &FilePath,
    loop_playback: bool,
) -> Result<axum::response::Response, ApiError> {
    use std::io::Read;

    let path_str = path
        .to_str()
        .ok_or_else(|| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Media path is not valid UTF-8",
            )
        })?
        .to_string();

    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-nostdin");

    if loop_playback {
        cmd.arg("-stream_loop").arg("-1");
    }

    cmd.arg("-re")
        .arg("-i")
        .arg(&path_str)
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg("0:a:0?")
        .arg("-c")
        .arg("copy")
        .arg("-f")
        .arg("mpegts")
        .arg("-mpegts_flags")
        .arg("+resend_headers")
        .arg("-");

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());

    let mut child = cmd.spawn().map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to start ffmpeg: {}", e),
        )
    })?;

    let mut stdout = child.stdout.take().ok_or_else(|| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to capture ffmpeg output",
        )
    })?;

    let _ = std::thread::spawn(move || {
        let _ = child.wait();
    });

    let (tx, mut rx) = tokio_mpsc::unbounded_channel::<Result<Vec<u8>, std::io::Error>>();

    std::thread::spawn(move || {
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

    let stream = async_stream::stream! {
        while let Some(chunk) = rx.recv().await {
            yield chunk;
        }
    };

    Ok((
        [("Content-Type", "video/mp2t")],
        axum::body::Body::from_stream(stream),
    )
        .into_response())
}

fn content_type_for_path(path: &FilePath) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp4") | Some("m4v") | Some("mov") => "video/mp4",
        Some("webm") => "video/webm",
        Some("ts") | Some("m2ts") | Some("mts") => "video/mp2t",
        Some("avi") => "video/x-msvideo",
        Some("mkv") => "video/x-matroska",
        _ => "application/octet-stream",
    }
}

async fn stream_media(
    State(app): State<WebAppState>,
    Path(filename): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    validate_media_filename(&filename)?;

    let media_path = app.media_dir.join(&filename);
    if !media_path.exists() {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "Media file not found",
        ));
    }

    let (encoder_pref, loop_playback) = {
        let st = app.shared.read().await;
        (st.preferred_encoder.clone(), st.loop_playback)
    };

    let hw_encoder = get_encoder_from_preference(&encoder_pref);
    log::info!(
        "Serving prepared media with encoder preference: {} -> {:?}, loop: {}",
        encoder_pref,
        hw_encoder,
        loop_playback
    );

    let prepared = prepare_media_for_dlna(&app.media_dir, &filename, &hw_encoder)
        .await
        .map_err(|e| error_response(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    if prepared.needs_loop_stream(loop_playback) {
        stream_copy_with_ffmpeg(prepared.path(), loop_playback)
    } else {
        serve_file_response(prepared.path(), prepared.content_type()).await
    }
}

fn remove_media_cache_files(media_dir: &PathBuf, filename: &str) {
    for profile in [CacheProfile::RemuxTs, CacheProfile::TranscodedH264Aac] {
        let cache_path = get_cache_path(media_dir, filename, profile);
        let _ = std::fs::remove_file(cache_path);
    }
}

async fn remove_upload_temp_file(path: &FilePath) {
    let _ = tokio::fs::remove_file(path).await;
}

/// POST /api/media/upload
async fn upload_media(
    State(app): State<WebAppState>,
    mut multipart: Multipart,
) -> Result<Json<UploadMediaResponse>, ApiError> {
    while let Some(mut field) = multipart.next_field().await.map_err(|e| {
        error_response(
            StatusCode::BAD_REQUEST,
            format!("Failed to parse multipart: {}", e),
        )
    })? {
        let field_name = field.name().unwrap_or("unknown");

        if field_name == "file" {
            let filename = field
                .file_name()
                .ok_or_else(|| error_response(StatusCode::BAD_REQUEST, "No filename provided"))?
                .to_string();

            validate_media_upload_filename(&filename)?;

            let dest_path = app.media_dir.join(&filename);
            if dest_path.exists() {
                return Err(error_response(
                    StatusCode::CONFLICT,
                    format!("Media file already exists: {}", filename),
                ));
            }

            let temp_path = app
                .media_dir
                .join(format!(".uploading-{}.tmp", Uuid::new_v4().simple()));

            use tokio::io::AsyncWriteExt;
            let mut file = tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)
                .await
                .map_err(|e| {
                    error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to create upload temp file: {}", e),
                    )
                })?;

            while let Some(chunk) = match field.chunk().await {
                Ok(chunk) => chunk,
                Err(e) => {
                    remove_upload_temp_file(&temp_path).await;
                    return Err(error_response(
                        StatusCode::BAD_REQUEST,
                        format!("Failed to read file: {}", e),
                    ));
                }
            } {
                if let Err(e) = file.write_all(&chunk).await {
                    remove_upload_temp_file(&temp_path).await;
                    return Err(error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to write file: {}", e),
                    ));
                }
            }

            if let Err(e) = file.flush().await {
                remove_upload_temp_file(&temp_path).await;
                return Err(error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to flush file: {}", e),
                ));
            }

            drop(file);

            tokio::fs::rename(&temp_path, &dest_path)
                .await
                .map_err(|e| {
                    let _ = std::fs::remove_file(&temp_path);
                    error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to move upload into place: {}", e),
                    )
                })?;

            let file = media_file_info(&app.media_dir, &filename)?;
            return Ok(Json(UploadMediaResponse { file }));
        }
    }

    Err(error_response(
        StatusCode::BAD_REQUEST,
        "No file field in multipart form",
    ))
}

async fn delete_media_file(
    State(app): State<WebAppState>,
    Path(filename): Path<String>,
) -> Result<StatusCode, ApiError> {
    validate_media_upload_filename(&filename)?;

    let path = app.media_dir.join(&filename);
    if !path.exists() {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "Media file not found",
        ));
    }

    tokio::fs::remove_file(&path).await.map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete media file: {}", e),
        )
    })?;
    remove_media_cache_files(&app.media_dir, &filename);

    let mut st = app.shared.write().await;
    for device in &mut st.devices {
        if device.current_media.as_deref() == Some(&filename) {
            device.current_media = None;
        }
    }
    for scene in &mut st.scenes {
        scene.assignments.retain(|_, media| media != &filename);
    }

    Ok(StatusCode::OK)
}

async fn rename_media_file(
    State(app): State<WebAppState>,
    Path(filename): Path<String>,
    Json(body): Json<RenameMediaRequest>,
) -> Result<Json<MediaFileInfo>, ApiError> {
    validate_media_upload_filename(&filename)?;
    validate_media_upload_filename(&body.new_name)?;

    if filename == body.new_name {
        return Ok(Json(media_file_info(&app.media_dir, &filename)?));
    }

    let old_path = app.media_dir.join(&filename);
    let new_path = app.media_dir.join(&body.new_name);
    if !old_path.exists() {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "Media file not found",
        ));
    }
    if new_path.exists() {
        return Err(error_response(
            StatusCode::CONFLICT,
            format!("Media file already exists: {}", body.new_name),
        ));
    }

    tokio::fs::rename(&old_path, &new_path).await.map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to rename media file: {}", e),
        )
    })?;
    remove_media_cache_files(&app.media_dir, &filename);
    remove_media_cache_files(&app.media_dir, &body.new_name);

    let mut st = app.shared.write().await;
    for device in &mut st.devices {
        if device.current_media.as_deref() == Some(&filename) {
            device.current_media = Some(body.new_name.clone());
        }
    }
    for scene in &mut st.scenes {
        for media in scene.assignments.values_mut() {
            if media == &filename {
                *media = body.new_name.clone();
            }
        }
    }
    drop(st);

    Ok(Json(media_file_info(&app.media_dir, &body.new_name)?))
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
        let media_uri = match prepare_media_uri(&app, &media_base, filename).await {
            Ok(uri) => uri,
            Err((_, Json(err))) => {
                results.push(SceneApplyResult {
                    device_uuid: uuid.clone(),
                    success: false,
                    error: Some(err.error),
                });
                continue;
            }
        };
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
    let valid = [
        "auto", "nvidia", "amd", "intel", "apple", "vaapi", "software",
    ];
    if !valid.contains(&body.encoder.as_str()) {
        return Err(error_response(StatusCode::BAD_REQUEST, "Invalid encoder"));
    }
    let mut st = app.shared.write().await;
    st.preferred_encoder = body.encoder;
    Ok(StatusCode::OK)
}

/// GET /api/config/encoders — get cached encoder detection result.
async fn get_encoders(State(app): State<WebAppState>) -> Json<Option<DetectionResult>> {
    let cached = app.cached_encoders.lock().await;
    Json(cached.clone())
}

/// GET /api/config/encoders/detect — trigger fresh encoder detection.
async fn detect_encoders(State(app): State<WebAppState>) -> Json<DetectionResult> {
    match detect_hw_encoders().await {
        Ok(result) => {
            let mut cached = app.cached_encoders.lock().await;
            *cached = Some(result.clone());
            Json(result)
        }
        Err(_e) => Json(DetectionResult {
            encoders: vec![],
            primary: None,
            detection_time_ms: 0,
            sources: vec![],
        }),
    }
}

#[derive(Deserialize)]
struct SetEncoderRequest {
    encoder: String,
}

#[derive(Deserialize)]
struct SetLoopRequest {
    loop_playback: bool,
}

async fn get_loop_playback(State(app): State<WebAppState>) -> Json<bool> {
    let st = app.shared.read().await;
    Json(st.loop_playback)
}

async fn set_loop_playback(
    State(app): State<WebAppState>,
    Json(body): Json<SetLoopRequest>,
) -> Result<StatusCode, ApiError> {
    let mut st = app.shared.write().await;
    st.loop_playback = body.loop_playback;
    log::info!("Loop playback set to: {}", st.loop_playback);
    Ok(StatusCode::OK)
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
    let loaded_config = config::load_or_create_config(generate_secret())
        .unwrap_or_else(|e| panic!("Failed to load ScreenPilot config: {}", e));

    if loaded_config.created {
        log::warn!(
            "Created default config at {:?}. The generated web UI password is stored in this TOML file.",
            loaded_config.path
        );
    } else {
        log::info!("Loaded config from {:?}", loaded_config.path);
    }

    let local_ip = media_server::local_ip().unwrap_or_else(|| "127.0.0.1".to_string());
    let media_base_url = format!("http://{}:8080", local_ip);

    log::info!(
        "Resolved local IP: {} (used for media server and DLNA multicast)",
        local_ip
    );

    let shared = state::new_shared_state();

    // Load saved devices
    {
        let mut s = shared.write().await;
        s.devices = persistence::load_devices();
        log::info!("Loaded {} devices from persistence", s.devices.len());
        s.media_server_base_url = media_base_url;
        s.preferred_encoder = "auto".to_string();
        s.loop_playback = true;
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
        cached_encoders: Arc::new(Mutex::new(None)),
        auth: Arc::new(AuthState::from_config(loaded_config.config.auth)),
    };

    // CORS — allow the Vue dev server and any other origin
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let auth_middleware_state = app_state.clone();
    let app = Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/status", get(auth_status))
        .route("/api/devices", get(get_devices))
        .route("/api/devices/discover", post(discover_devices))
        .route("/api/devices/:uuid/alias", put(set_device_alias))
        .route("/api/devices/:uuid/play", post(play_on_device))
        .route("/api/devices/:uuid/pause", post(pause_device))
        .route("/api/devices/:uuid/stop", post(stop_device))
        .route(
            "/api/devices/:uuid/loop",
            get(get_device_loop).put(set_device_loop),
        )
        .route("/api/media", get(list_media))
        .route("/api/media/files", get(list_media_files))
        .route("/api/media/files/:filename", delete(delete_media_file))
        .route("/api/media/files/:filename/rename", put(rename_media_file))
        .route(
            "/api/media/upload",
            post(upload_media).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
        .route("/api/media/stream/*path", get(stream_media))
        .route("/api/scenes", get(get_scenes).post(save_scene))
        .route("/api/scenes/:name", delete(delete_scene))
        .route("/api/scenes/:name/apply", post(apply_scene))
        .route("/api/config/media-server-url", get(get_media_server_url))
        .route("/api/config/encoder", get(get_encoder).put(set_encoder))
        .route("/api/config/encoders", get(get_encoders))
        .route("/api/config/encoders/detect", get(detect_encoders))
        .route(
            "/api/config/loop-playback",
            get(get_loop_playback).put(set_loop_playback),
        )
        .route("/web/assets/*path", get(serve_assets))
        .route("/web/favicon.ico", get(serve_favicon))
        .route("/web", get(serve_frontend))
        .route("/web/", get(serve_frontend))
        .fallback(get(serve_frontend))
        .layer(cors)
        .layer(middleware::from_fn_with_state(
            auth_middleware_state,
            require_auth,
        ))
        .with_state(app_state);

    let (listener, port) = bind_with_fallback(8080).await;

    log::info!(
        "ScreenPilot API server listening on http://0.0.0.0:{}",
        port
    );
    log::info!("Web UI: http://localhost:{}/web", port);

    axum::serve(listener, app).await.expect("API server error");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_state_issues_and_revokes_session() {
        let auth = AuthState::from_config(config::AuthSettings {
            username: "admin".to_string(),
            password: "secret".to_string(),
        });

        assert!(auth.verify_credentials("admin", "secret"));
        assert!(!auth.verify_credentials("admin", "wrong"));

        let token = auth.issue_session_token();
        assert!(auth.verify_session_token(&token));

        auth.revoke_session_token(&token);
        assert!(!auth.verify_session_token(&token));
    }

    #[test]
    fn test_session_token_from_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("theme=dark; screenpilot_session=abc123; other=value"),
        );

        assert_eq!(session_token_from_headers(&headers), Some("abc123"));
    }

    #[test]
    fn test_normalize_device_alias_trims_and_clears_empty() {
        assert_eq!(
            normalize_device_alias(Some("  Lobby Screen  ".to_string())).unwrap(),
            Some("Lobby Screen".to_string())
        );
        assert_eq!(
            normalize_device_alias(Some("   ".to_string())).unwrap(),
            None
        );
        assert_eq!(normalize_device_alias(None).unwrap(), None);
    }

    #[test]
    fn test_normalize_device_alias_rejects_too_long() {
        let alias = "a".repeat(MAX_DEVICE_ALIAS_CHARS + 1);
        let result = normalize_device_alias(Some(alias));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_media_upload_filename() {
        assert_eq!(
            validate_media_upload_filename("promo.mp4").unwrap(),
            "mp4".to_string()
        );
        assert!(validate_media_upload_filename(".hidden.mp4").is_err());
        assert!(validate_media_upload_filename("promo.txt").is_err());
        assert!(validate_media_upload_filename("nested/promo.mp4").is_err());
    }

    #[test]
    fn test_media_file_info_includes_size_and_extension() {
        let temp_dir = tempfile::tempdir().unwrap();
        let media_dir = temp_dir.path().to_path_buf();
        std::fs::write(media_dir.join("promo.mp4"), b"hello").unwrap();

        let info = media_file_info(&media_dir, "promo.mp4").unwrap();

        assert_eq!(info.name, "promo.mp4");
        assert_eq!(info.size, 5);
        assert_eq!(info.extension, "mp4");
        assert!(info.modified.is_some());
    }

    #[test]
    fn test_get_cache_dir() {
        let media_dir = PathBuf::from("/test/media");
        let cache_dir = get_cache_dir(&media_dir);
        assert_eq!(cache_dir, PathBuf::from("/test/media/.cache"));
    }

    #[test]
    fn test_get_cache_path() {
        let media_dir = PathBuf::from("/test/media");

        let path = get_cache_path(&media_dir, "video.mp4", CacheProfile::TranscodedH264Aac);
        assert!(path
            .to_string_lossy()
            .contains("video.mp4.dlna-h264-aac-v2.ts"));

        let path_webm = get_cache_path(&media_dir, "test.webm", CacheProfile::RemuxTs);
        assert!(path_webm
            .to_string_lossy()
            .contains("test.webm.dlna-remux-ts-v1.ts"));

        let path_mkv = get_cache_path(&media_dir, "movie.mkv", CacheProfile::TranscodedH264Aac);
        assert!(path_mkv
            .to_string_lossy()
            .contains("movie.mkv.dlna-h264-aac-v2.ts"));
    }

    #[test]
    fn test_get_cache_path_sanitizes_filename() {
        let media_dir = PathBuf::from("/test/media");

        let path = get_cache_path(
            &media_dir,
            "video/with:invalid*chars.mp4",
            CacheProfile::TranscodedH264Aac,
        );
        let path_str = path.to_string_lossy();

        let has_invalid_chars =
            path_str.contains(':') || path_str.contains('*') || path_str.contains('?');
        assert!(
            !has_invalid_chars,
            "Path should not contain invalid chars: {}",
            path_str
        );
    }

    #[test]
    fn test_check_cache_nonexistent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let media_dir = temp_dir.path().to_path_buf();

        let result = check_cache(
            &media_dir,
            "nonexistent.mp4",
            CacheProfile::TranscodedH264Aac,
        );
        assert!(result.is_none());

        drop(temp_dir);
    }

    #[test]
    fn test_check_cache_file_too_old() {
        let temp_dir = tempfile::tempdir().unwrap();
        let media_dir = temp_dir.path().to_path_buf();

        std::fs::write(media_dir.join("original.mp4"), b"original").unwrap();
        let cache_dir = media_dir.join(".cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(
            cache_dir.join("original.mp4.dlna-h264-aac-v2.ts"),
            b"cached",
        )
        .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(100));
        std::fs::write(media_dir.join("original.mp4"), b"newer").unwrap();

        let result = check_cache(&media_dir, "original.mp4", CacheProfile::TranscodedH264Aac);
        assert!(result.is_none());

        drop(temp_dir);
    }

    #[test]
    fn test_check_cache_valid() {
        let temp_dir = tempfile::tempdir().unwrap();
        let media_dir = temp_dir.path().to_path_buf();

        std::fs::write(media_dir.join("original.mp4"), b"original").unwrap();

        std::thread::sleep(std::time::Duration::from_millis(100));

        let cache_dir = media_dir.join(".cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(
            cache_dir.join("original.mp4.dlna-h264-aac-v2.ts"),
            b"cached",
        )
        .unwrap();

        let result = check_cache(&media_dir, "original.mp4", CacheProfile::TranscodedH264Aac);
        assert!(result.is_some());
        assert!(result.unwrap().to_string_lossy().contains(".cache"));

        drop(temp_dir);
    }

    #[tokio::test]
    async fn test_cache_job_lock_deduplicates_concurrent_creators() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let temp_dir = tempfile::tempdir().unwrap();
        let media_dir = temp_dir.path().to_path_buf();
        std::fs::write(media_dir.join("shared.mp4"), b"original").unwrap();

        std::thread::sleep(std::time::Duration::from_millis(100));

        let creator_calls = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(tokio::sync::Barrier::new(6));
        let mut tasks = Vec::new();

        for _ in 0..6 {
            let media_dir = media_dir.clone();
            let creator_calls = Arc::clone(&creator_calls);
            let barrier = Arc::clone(&barrier);

            tasks.push(tokio::spawn(async move {
                barrier.wait().await;

                with_cache_job_lock(
                    &media_dir,
                    "shared.mp4",
                    CacheProfile::TranscodedH264Aac,
                    || {
                        let media_dir = media_dir.clone();
                        let creator_calls = Arc::clone(&creator_calls);

                        async move {
                            creator_calls.fetch_add(1, Ordering::SeqCst);
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

                            let cache_path = get_cache_path(
                                &media_dir,
                                "shared.mp4",
                                CacheProfile::TranscodedH264Aac,
                            );
                            std::fs::create_dir_all(get_cache_dir(&media_dir)).unwrap();
                            std::fs::write(&cache_path, b"cached").unwrap();

                            Ok(cache_path)
                        }
                    },
                )
                .await
            }));
        }

        for task in tasks {
            let path = task.await.unwrap().unwrap();
            assert_eq!(
                path,
                get_cache_path(&media_dir, "shared.mp4", CacheProfile::TranscodedH264Aac)
            );
        }

        assert_eq!(creator_calls.load(Ordering::SeqCst), 1);

        drop(temp_dir);
    }

    fn dlna_safe_media_info() -> MediaInfo {
        MediaInfo {
            format_name: Some("mov,mp4,m4a,3gp,3g2,mj2".to_string()),
            video_codec: Some("h264".to_string()),
            video_profile: Some("Main".to_string()),
            video_level: Some(40),
            video_pix_fmt: Some("yuv420p".to_string()),
            audio_codec: Some("aac".to_string()),
            video_width: Some(1920),
            video_height: Some(1080),
            audio_channels: Some(2),
            audio_sample_rate: Some(48_000),
        }
    }

    #[test]
    fn test_dlna_bypass_accepts_safe_mp4() {
        let info = dlna_safe_media_info();
        assert!(is_dlna_bypass_compatible(&info, "ad.mp4"));
    }

    #[test]
    fn test_dlna_bypass_rejects_high_level_h264() {
        let mut info = dlna_safe_media_info();
        info.video_level = Some(52);
        assert!(!is_dlna_bypass_compatible(&info, "ad.mp4"));
    }

    #[test]
    fn test_dlna_bypass_rejects_webm_container() {
        let mut info = dlna_safe_media_info();
        info.format_name = Some("matroska,webm".to_string());
        assert!(!is_dlna_bypass_compatible(&info, "ad.webm"));
        assert!(has_safe_dlna_codecs(&info));
    }

    #[test]
    fn test_remux_cache_is_separate_from_transcode_cache() {
        let media_dir = PathBuf::from("/test/media");
        let remux = get_cache_path(&media_dir, "ad.webm", CacheProfile::RemuxTs);
        let transcode = get_cache_path(&media_dir, "ad.webm", CacheProfile::TranscodedH264Aac);

        assert_ne!(remux, transcode);
        assert!(remux.to_string_lossy().contains("dlna-remux-ts-v1"));
        assert!(transcode.to_string_lossy().contains("dlna-h264-aac-v2"));
    }

    #[test]
    fn test_percent_encode_path_segment() {
        assert_eq!(
            percent_encode_path_segment("promo loop 01.mp4"),
            "promo%20loop%2001.mp4"
        );
        assert_eq!(
            percent_encode_path_segment("菜单.mp4"),
            "%E8%8F%9C%E5%8D%95.mp4"
        );
    }

    #[test]
    fn test_hardware_encoder_detection() {
        let detected = detect_hardware_encoder();

        match detected {
            HardwareEncoder::None => println!("Using software encoder"),
            HardwareEncoder::Nvidia => println!("Using NVIDIA"),
            HardwareEncoder::IntelQsv => println!("Using Intel QSV"),
            HardwareEncoder::AmdVce => println!("Using AMD VCE"),
            HardwareEncoder::AppleVtb => println!("Using Apple VT"),
            HardwareEncoder::Vaapi => println!("Using VAAPI"),
        }

        assert!(true);
    }

    #[test]
    fn test_encoder_from_preference() {
        assert!(matches!(
            get_encoder_from_preference("software"),
            HardwareEncoder::None
        ));
        assert!(matches!(
            get_encoder_from_preference("nvidia"),
            HardwareEncoder::Nvidia
        ));
        assert!(matches!(
            get_encoder_from_preference("amd"),
            HardwareEncoder::AmdVce
        ));
        assert!(matches!(
            get_encoder_from_preference("intel"),
            HardwareEncoder::IntelQsv
        ));
        assert!(matches!(
            get_encoder_from_preference("apple"),
            HardwareEncoder::AppleVtb
        ));
        assert!(matches!(
            get_encoder_from_preference("vaapi"),
            HardwareEncoder::Vaapi
        ));
    }

    #[test]
    fn test_should_fallback_to_software_for_hardware_encoders() {
        assert!(!should_fallback_to_software(&HardwareEncoder::None));
        assert!(should_fallback_to_software(&HardwareEncoder::Nvidia));
        assert!(should_fallback_to_software(&HardwareEncoder::AmdVce));
        assert!(should_fallback_to_software(&HardwareEncoder::IntelQsv));
        assert!(should_fallback_to_software(&HardwareEncoder::AppleVtb));
        assert!(should_fallback_to_software(&HardwareEncoder::Vaapi));
    }

    #[test]
    fn test_software_fallback_failure_message_includes_both_errors() {
        let message = software_fallback_failure_message(
            &HardwareEncoder::Nvidia,
            "nvenc initialization failed",
            "libx264 failed",
        );

        assert!(message.contains("Nvidia"));
        assert!(message.contains("nvenc initialization failed"));
        assert!(message.contains("software fallback failed"));
        assert!(message.contains("libx264 failed"));
    }
}
