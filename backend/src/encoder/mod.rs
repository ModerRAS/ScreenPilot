use serde::{Deserialize, Serialize};
use std::time::Instant;

mod ffmpeg;
mod gpu;
mod os_framework;
mod runtime_test;

/// Backend type for hardware encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EncoderBackend {
    /// NVIDIA NVENC
    Nvenc,
    /// Intel Quick Sync Video
    Qsv,
    /// Video Acceleration API (Linux)
    Vaapi,
    /// AMD Advanced Media Framework
    Amf,
    /// Apple VideoToolbox
    Videotoolbox,
    /// Windows Media Foundation
    Mf,
    /// Software encoding (fallback)
    Software,
}

impl std::fmt::Display for EncoderBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncoderBackend::Nvenc => write!(f, "nvenc"),
            EncoderBackend::Qsv => write!(f, "qsv"),
            EncoderBackend::Vaapi => write!(f, "vaapi"),
            EncoderBackend::Amf => write!(f, "amf"),
            EncoderBackend::Videotoolbox => write!(f, "videotoolbox"),
            EncoderBackend::Mf => write!(f, "mf"),
            EncoderBackend::Software => write!(f, "software"),
        }
    }
}

/// Supported video codecs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    H264,
    Hevc,
    Av1,
    Vp9,
}

impl std::fmt::Display for VideoCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VideoCodec::H264 => write!(f, "h264"),
            VideoCodec::Hevc => write!(f, "hevc"),
            VideoCodec::Av1 => write!(f, "av1"),
            VideoCodec::Vp9 => write!(f, "vp9"),
        }
    }
}

/// Represents a hardware video encoder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwEncoder {
    /// Video codec (h264, hevc, av1, vp9)
    pub codec: VideoCodec,
    /// Backend type (nvenc, qsv, vaapi, etc.)
    pub backend: EncoderBackend,
    /// Device identifier (e.g., "NVIDIA GeForce RTX 3080")
    pub device: Option<String>,
    /// Priority: 1 = highest (native GPU API), 2 = OS framework, 3 = FFmpeg
    pub priority: u8,
    /// FFmpeg encoder name (e.g., "h264_nvenc")
    pub ffmpeg_name: String,
}

/// Detection source layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DetectionSource {
    /// L1: Native GPU vendor APIs (NVML, AMF, etc.)
    L1GpuApi,
    /// L2: OS media frameworks (VideoToolbox, VAAPI, MF)
    L2OsFramework,
    /// L3: FFmpeg encoder probing
    L3Ffmpeg,
    /// L4: Runtime validation
    L4Runtime,
}

/// Result of hardware encoder detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    /// All detected encoders
    pub encoders: Vec<HwEncoder>,
    /// Primary (best) encoder recommendation
    pub primary: Option<HwEncoder>,
    /// Time taken for detection in milliseconds
    pub detection_time_ms: u64,
    /// Sources that were probed during detection
    pub sources: Vec<DetectionSource>,
}

/// Detect available hardware video encoders on the system.
///
/// This function implements multi-layer detection:
/// - L1: Native GPU vendor APIs
/// - L2: OS media frameworks
/// - L3: FFmpeg encoder probing
///
/// Returns a `DetectionResult` containing all detected encoders and metadata.
pub async fn detect_hw_encoders() -> anyhow::Result<DetectionResult> {
    let start = Instant::now();
    let mut encoders = Vec::new();
    let mut sources: Vec<DetectionSource> = Vec::new();

    log::info!("[encoder] Starting multi-layer hardware encoder detection");

    log::info!("[encoder] L1: Starting GPU native API detection...");
    match gpu::detect_gpu_encoders() {
        Ok(gpu_encoders) => {
            if !gpu_encoders.is_empty() {
                sources.push(DetectionSource::L1GpuApi);
                encoders.extend(gpu_encoders);
                log::info!("[encoder] L1: GPU detection found {} encoders", encoders.len());
            } else {
                log::info!("[encoder] L1: No hardware encoders found via GPU APIs");
            }
        }
        Err(e) => {
            log::warn!("[encoder] L1: GPU probing failed: {}", e);
        }
    }

    // L2: OS framework detection
    log::info!("[encoder] L2: Starting OS framework detection...");
    match os_framework::detect_os_framework_encoders() {
        Ok(os_encoders) => {
            if !os_encoders.is_empty() {
                sources.push(DetectionSource::L2OsFramework);
                encoders.extend(os_encoders);
                log::info!("[encoder] L2: OS framework detection found {} encoders", encoders.len());
            } else {
                log::info!("[encoder] L2: No hardware encoders found via OS frameworks");
            }
        }
        Err(e) => {
            log::warn!("[encoder] L2: OS framework probing failed: {}", e);
        }
    }

    // L3: FFmpeg probing in ffmpeg.rs
    log::info!("[encoder] L3: Starting FFmpeg encoder probing...");
    match ffmpeg::probe_ffmpeg_encoders() {
        Ok(ffmpeg_encoders) => {
            if !ffmpeg_encoders.is_empty() {
                sources.push(DetectionSource::L3Ffmpeg);
                encoders.extend(ffmpeg_encoders);
                log::info!("[encoder] L3: FFmpeg detection found {} encoders", encoders.len());
            } else {
                log::info!("[encoder] L3: No hardware encoders found via FFmpeg");
            }
        }
        Err(e) => {
            log::warn!("[encoder] L3: FFmpeg probing failed: {}", e);
        }
    }

    if runtime_test::is_l4_enabled() {
        log::info!("[encoder] L4: Starting runtime validation...");
        sources.push(DetectionSource::L4Runtime);
        encoders = runtime_test::validate_encoders_runtime(&encoders).await;
    }

    // TODO: Implement multi-layer detection:
    // - L1: GPU native API detection in gpu.rs
    // - L2: OS framework detection in os_framework.rs

    // If no encoders detected, return software fallback
    if encoders.is_empty() {
        log::warn!("[encoder] No hardware encoders detected, using software fallback");
        encoders.push(HwEncoder {
            codec: VideoCodec::H264,
            backend: EncoderBackend::Software,
            device: None,
            priority: 10,
            ffmpeg_name: "libx264".to_string(),
        });
    }

    // Select primary encoder (highest priority)
    let primary = encoders
        .iter()
        .min_by_key(|e| e.priority)
        .cloned();

    if let Some(ref primary) = primary {
        log::info!(
            "[encoder] Primary encoder: {} ({})",
            primary.ffmpeg_name,
            primary.backend
        );
    }

    let detection_time_ms = start.elapsed().as_millis() as u64;
    log::info!(
        "[encoder] Detection complete: {} encoders found in {}ms",
        encoders.len(),
        detection_time_ms
    );

    Ok(DetectionResult {
        encoders,
        primary,
        detection_time_ms,
        sources,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoder_backend_display() {
        assert_eq!(EncoderBackend::Nvenc.to_string(), "nvenc");
        assert_eq!(EncoderBackend::Qsv.to_string(), "qsv");
        assert_eq!(EncoderBackend::Software.to_string(), "software");
    }

    #[test]
    fn test_video_codec_display() {
        assert_eq!(VideoCodec::H264.to_string(), "h264");
        assert_eq!(VideoCodec::Hevc.to_string(), "hevc");
    }

    #[tokio::test]
    async fn test_detect_hw_encoders() {
        let result = detect_hw_encoders().await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(!result.encoders.is_empty());
        assert!(result.primary.is_some());
        assert!(result.detection_time_ms > 0);
    }
}
