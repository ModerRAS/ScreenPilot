//! FFmpeg-based hardware encoder detection.
//!
//! This module provides FFmpeg binary detection and encoder probing
//! for hardware-accelerated video encoding support.

use crate::encoder::{EncoderBackend, HwEncoder, VideoCodec};
use std::path::{Path, PathBuf};

/// Find the FFmpeg binary on the system.
///
/// Searches in the following order:
/// 1. Current directory (./ffmpeg, ./ffmpeg.exe)
/// 2. Project root (relative to CARGO_MANIFEST_DIR)
/// 3. System PATH
/// 4. Common system paths (Windows: C:\ffmpeg\bin, Linux: /usr/bin, /usr/local/bin)
///
/// Returns the path to FFmpeg or an error if not found.
pub fn find_ffmpeg() -> anyhow::Result<PathBuf> {
    log::info!("[encoder] L3: probing FFmpeg...");

    // 1. Check current directory
    let current_dir = std::env::current_dir()?;
    let ffmpeg_name = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };

    let current_ffmpeg = current_dir.join(ffmpeg_name);
    if current_ffmpeg.exists() {
        log::info!(
            "[encoder] L3: found FFmpeg in current directory: {:?}",
            current_ffmpeg
        );
        return Ok(current_ffmpeg);
    }

    // 2. Check project root (relative to CARGO_MANIFEST_DIR)
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let project_root = Path::new(&manifest_dir)
            .parent()
            .unwrap_or(Path::new(&manifest_dir));
        let project_ffmpeg = project_root.join(ffmpeg_name);
        if project_ffmpeg.exists() {
            log::info!(
                "[encoder] L3: found FFmpeg in project root: {:?}",
                project_ffmpeg
            );
            return Ok(project_ffmpeg);
        }
    }

    // 3. Check system PATH
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let path_ffmpeg = dir.join(ffmpeg_name);
            if path_ffmpeg.exists() {
                log::info!("[encoder] L3: found FFmpeg in PATH: {:?}", path_ffmpeg);
                return Ok(path_ffmpeg);
            }
        }
    }

    // 4. Check common system paths
    let system_paths = if cfg!(target_os = "windows") {
        vec![
            PathBuf::from(r"C:\ffmpeg\bin\ffmpeg.exe"),
            PathBuf::from(r"C:\Program Files\ffmpeg\bin\ffmpeg.exe"),
            PathBuf::from(r"C:\Program Files (x86)\ffmpeg\bin\ffmpeg.exe"),
        ]
    } else if cfg!(target_os = "macos") {
        vec![
            PathBuf::from("/usr/local/bin/ffmpeg"),
            PathBuf::from("/opt/homebrew/bin/ffmpeg"),
            PathBuf::from("/usr/bin/ffmpeg"),
        ]
    } else {
        // Linux and others
        vec![
            PathBuf::from("/usr/bin/ffmpeg"),
            PathBuf::from("/usr/local/bin/ffmpeg"),
            PathBuf::from("/opt/ffmpeg/bin/ffmpeg"),
        ]
    };

    for path in system_paths {
        if path.exists() {
            log::info!("[encoder] L3: found FFmpeg in system path: {:?}", path);
            return Ok(path);
        }
    }

    anyhow::bail!("FFmpeg not found in any searched location")
}

/// Hardware encoder information parsed from FFmpeg output.
#[derive(Debug)]
struct FfmpegEncoderInfo {
    /// FFmpeg encoder name (e.g., "h264_nvenc")
    ffmpeg_name: String,
    /// Codec name (e.g., "h264")
    #[allow(dead_code)]
    codec_name: String,
    /// Whether this is a hardware encoder
    #[allow(dead_code)]
    is_hardware: bool,
}

/// Map FFmpeg encoder name to our HwEncoder struct.
fn map_ffmpeg_encoder(info: &FfmpegEncoderInfo) -> Option<HwEncoder> {
    let (codec, backend) = match info.ffmpeg_name.as_str() {
        // NVIDIA NVENC
        "h264_nvenc" => (VideoCodec::H264, EncoderBackend::Nvenc),
        "hevc_nvenc" => (VideoCodec::Hevc, EncoderBackend::Nvenc),
        "av1_nvenc" => (VideoCodec::Av1, EncoderBackend::Nvenc),

        // Intel QSV
        "h264_qsv" => (VideoCodec::H264, EncoderBackend::Qsv),
        "hevc_qsv" => (VideoCodec::Hevc, EncoderBackend::Qsv),
        "av1_qsv" => (VideoCodec::Av1, EncoderBackend::Qsv),

        // AMD AMF
        "h264_amf" => (VideoCodec::H264, EncoderBackend::Amf),
        "hevc_amf" => (VideoCodec::Hevc, EncoderBackend::Amf),
        "av1_amf" => (VideoCodec::Av1, EncoderBackend::Amf),

        // Apple VideoToolbox
        "h264_videotoolbox" => (VideoCodec::H264, EncoderBackend::Videotoolbox),
        "hevc_videotoolbox" => (VideoCodec::Hevc, EncoderBackend::Videotoolbox),
        "av1_videotoolbox" => (VideoCodec::Av1, EncoderBackend::Videotoolbox),

        // VAAPI (Linux)
        "h264_vaapi" => (VideoCodec::H264, EncoderBackend::Vaapi),
        "hevc_vaapi" => (VideoCodec::Hevc, EncoderBackend::Vaapi),
        "av1_vaapi" => (VideoCodec::Av1, EncoderBackend::Vaapi),

        // Not a hardware encoder we're tracking
        _ => return None,
    };

    // Priority: 3 = FFmpeg layer (as per spec: 1 = native GPU, 2 = OS framework, 3 = FFmpeg)
    Some(HwEncoder {
        codec,
        backend,
        device: None, // FFmpeg doesn't provide device info in encoder list
        priority: 3,
        ffmpeg_name: info.ffmpeg_name.clone(),
    })
}

/// Parse FFmpeg encoder output to extract hardware encoder info.
///
/// Output format: "V....D h264_vaapi   H.264/AVC (VAAPI)"
fn parse_encoder_line(line: &str) -> Option<FfmpegEncoderInfo> {
    let line = line.trim();

    if line.is_empty() || line.starts_with("Encoders") || line.starts_with("----") {
        return None;
    }

    let hw_encoders = [
        "h264_nvenc",
        "hevc_nvenc",
        "av1_nvenc",
        "h264_qsv",
        "hevc_qsv",
        "av1_qsv",
        "h264_amf",
        "hevc_amf",
        "av1_amf",
        "h264_videotoolbox",
        "hevc_videotoolbox",
        "av1_videotoolbox",
        "h264_vaapi",
        "hevc_vaapi",
        "av1_vaapi",
    ];

    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let encoder_token = if hw_encoders.contains(&parts[0]) {
        parts[0]
    } else if parts.len() >= 2 && hw_encoders.contains(&parts[1]) {
        parts[1]
    } else {
        return None;
    };

    let codec_name = encoder_token.split('_').next().unwrap_or(encoder_token);

    Some(FfmpegEncoderInfo {
        ffmpeg_name: encoder_token.to_string(),
        codec_name: codec_name.to_string(),
        is_hardware: true,
    })
}

/// Probe FFmpeg for available hardware encoders.
///
/// Executes `ffmpeg -hide_banner -encoders` and parses the output
/// to detect supported hardware encoders.
///
/// Returns a vector of detected hardware encoders.
pub fn probe_ffmpeg_encoders() -> anyhow::Result<Vec<HwEncoder>> {
    log::info!("[encoder] L3: probing FFmpeg encoders...");

    let ffmpeg_path = find_ffmpeg()?;

    let output = std::process::Command::new(&ffmpeg_path)
        .arg("-hide_banner")
        .arg("-encoders")
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute FFmpeg: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("FFmpeg encoder probe failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut encoders = Vec::new();

    for line in stdout.lines() {
        if let Some(info) = parse_encoder_line(line) {
            if let Some(hw_encoder) = map_ffmpeg_encoder(&info) {
                log::info!(
                    "[encoder] L3: detected hardware encoder: {} ({})",
                    hw_encoder.ffmpeg_name,
                    hw_encoder.backend
                );
                encoders.push(hw_encoder);
            }
        }
    }

    log::info!(
        "[encoder] L3: FFmpeg probe complete, found {} hardware encoders",
        encoders.len()
    );

    Ok(encoders)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_encoder_line_nvenc() {
        let line = " V....D h264_nvenc           NVIDIA NVENC H.264 encoder [NVIDIA GeForce RTX 3080]";
        let info = parse_encoder_line(line).unwrap();
        assert_eq!(info.ffmpeg_name, "h264_nvenc");
        assert_eq!(info.codec_name, "h264");
        assert!(info.is_hardware);
    }

    #[test]
    fn test_parse_encoder_line_qsv() {
        let line = " V..... hevc_qsv             Intel Quick Sync Video HEVC encoder [Intel Iris Xe]";
        let info = parse_encoder_line(line).unwrap();
        assert_eq!(info.ffmpeg_name, "hevc_qsv");
        assert_eq!(info.codec_name, "hevc");
    }

    #[test]
    fn test_parse_encoder_line_videotoolbox() {
        let line = " V....D h264_videotoolbox    Apple VideoToolbox H.264 encoder";
        let info = parse_encoder_line(line).unwrap();
        assert_eq!(info.ffmpeg_name, "h264_videotoolbox");
    }

    #[test]
    fn test_parse_encoder_line_vaapi() {
        let line = " V....D hevc_vaapi           VAAPI HEVC encoder";
        let info = parse_encoder_line(line).unwrap();
        assert_eq!(info.ffmpeg_name, "hevc_vaapi");
    }

    #[test]
    fn test_parse_encoder_line_software() {
        let line = " V....D libx264              libx264 H.264 encoder";
        let info = parse_encoder_line(line);
        assert!(info.is_none());
    }

    #[test]
    fn test_map_ffmpeg_encoder_nvenc() {
        let info = FfmpegEncoderInfo {
            ffmpeg_name: "h264_nvenc".to_string(),
            codec_name: "h264".to_string(),
            is_hardware: true,
        };
        let encoder = map_ffmpeg_encoder(&info).unwrap();
        assert_eq!(encoder.codec, VideoCodec::H264);
        assert_eq!(encoder.backend, EncoderBackend::Nvenc);
        assert_eq!(encoder.ffmpeg_name, "h264_nvenc");
        assert_eq!(encoder.priority, 3);
    }

    #[test]
    fn test_map_ffmpeg_encoder_qsv() {
        let info = FfmpegEncoderInfo {
            ffmpeg_name: "av1_qsv".to_string(),
            codec_name: "av1".to_string(),
            is_hardware: true,
        };
        let encoder = map_ffmpeg_encoder(&info).unwrap();
        assert_eq!(encoder.codec, VideoCodec::Av1);
        assert_eq!(encoder.backend, EncoderBackend::Qsv);
    }

    #[test]
    fn test_map_ffmpeg_encoder_unknown() {
        let info = FfmpegEncoderInfo {
            ffmpeg_name: "libx264".to_string(),
            codec_name: "h264".to_string(),
            is_hardware: false,
        };
        let encoder = map_ffmpeg_encoder(&info);
        assert!(encoder.is_none());
    }
}
