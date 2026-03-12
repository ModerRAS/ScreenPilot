use crate::encoder::{EncoderBackend, HwEncoder, VideoCodec};

pub fn detect_os_framework_encoders() -> anyhow::Result<Vec<HwEncoder>> {
    log::info!("[encoder] L2: probing OS media frameworks...");

    #[cfg(target_os = "windows")]
    {
        detect_windows_media_foundation()
    }

    #[cfg(target_os = "macos")]
    {
        detect_macos_videotoolbox()
    }

    #[cfg(target_os = "linux")]
    {
        detect_linux_vaapi()
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        log::info!("[encoder] L2: unsupported OS for framework detection");
        Ok(Vec::new())
    }
}

#[cfg(target_os = "windows")]
fn detect_windows_media_foundation() -> anyhow::Result<Vec<HwEncoder>> {
    use std::process::Command;

    log::info!("[encoder] L2: probing Windows Media Foundation...");

    let mut encoders = Vec::new();

    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.contains("h264_mf") {
            log::info!("[encoder] L2: found h264_mf (Windows Media Foundation)");
            encoders.push(HwEncoder {
                codec: VideoCodec::H264,
                backend: EncoderBackend::Mf,
                device: Some("Windows Media Foundation".to_string()),
                priority: 2,
                ffmpeg_name: "h264_mf".to_string(),
            });
        }

        if stdout.contains("hevc_mf") {
            log::info!("[encoder] L2: found hevc_mf (Windows Media Foundation)");
            encoders.push(HwEncoder {
                codec: VideoCodec::Hevc,
                backend: EncoderBackend::Mf,
                device: Some("Windows Media Foundation".to_string()),
                priority: 2,
                ffmpeg_name: "hevc_mf".to_string(),
            });
        }
    }

    if encoders.is_empty() {
        log::info!("[encoder] L2: checking Windows registry for MF encoders...");

        let reg_output = Command::new("reg")
            .args([
                "query",
                "HKLM\\SOFTWARE\\Microsoft\\Windows Media Foundation\\HardwareEncoder",
                "/v",
                "EMSearchFilterCLSID",
            ])
            .output();

        if let Ok(output) = reg_output {
            if output.status.success() {
                log::info!("[encoder] L2: Windows Media Foundation hardware encoders available");

                encoders.push(HwEncoder {
                    codec: VideoCodec::H264,
                    backend: EncoderBackend::Mf,
                    device: Some("Windows Media Foundation (Hardware)".to_string()),
                    priority: 2,
                    ffmpeg_name: "h264_mf".to_string(),
                });

                encoders.push(HwEncoder {
                    codec: VideoCodec::Hevc,
                    backend: EncoderBackend::Mf,
                    device: Some("Windows Media Foundation (Hardware)".to_string()),
                    priority: 2,
                    ffmpeg_name: "hevc_mf".to_string(),
                });
            }
        }
    }

    if encoders.is_empty() {
        log::info!("[encoder] L2: no Windows Media Foundation encoders found");
    } else {
        log::info!(
            "[encoder] L2: Windows Media Foundation found {} encoders",
            encoders.len()
        );
    }

    Ok(encoders)
}

#[cfg(target_os = "macos")]
fn detect_macos_videotoolbox() -> anyhow::Result<Vec<HwEncoder>> {
    use std::process::Command;

    log::info!("[encoder] L2: probing macOS VideoToolbox...");

    let mut encoders = Vec::new();

    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.contains("h264_videotoolbox") {
            log::info!("[encoder] L2: found h264_videotoolbox (Apple VideoToolbox)");
            encoders.push(HwEncoder {
                codec: VideoCodec::H264,
                backend: EncoderBackend::Videotoolbox,
                device: Some("Apple VideoToolbox".to_string()),
                priority: 2,
                ffmpeg_name: "h264_videotoolbox".to_string(),
            });
        }

        if stdout.contains("hevc_videotoolbox") {
            log::info!("[encoder] L2: found hevc_videotoolbox (Apple VideoToolbox)");
            encoders.push(HwEncoder {
                codec: VideoCodec::Hevc,
                backend: EncoderBackend::Videotoolbox,
                device: Some("Apple VideoToolbox".to_string()),
                priority: 2,
                ffmpeg_name: "hevc_videotoolbox".to_string(),
            });
        }

        if stdout.contains("av1_videotoolbox") {
            log::info!("[encoder] L2: found av1_videotoolbox (Apple VideoToolbox)");
            encoders.push(HwEncoder {
                codec: VideoCodec::Av1,
                backend: EncoderBackend::Videotoolbox,
                device: Some("Apple VideoToolbox".to_string()),
                priority: 2,
                ffmpeg_name: "av1_videotoolbox".to_string(),
            });
        }
    }

    if !encoders.is_empty() {
        let _ = Command::new("system_profiler")
            .args(["SPDisplaysDataType"])
            .output()
            .map(|o| {
                if o.status.success() {
                    log::info!("[encoder] L2: macOS GPU detected, VideoToolbox hardware encoding available");
                }
            });
    }

    if encoders.is_empty() {
        log::info!("[encoder] L2: no VideoToolbox encoders found");
    } else {
        log::info!(
            "[encoder] L2: VideoToolbox found {} encoders",
            encoders.len()
        );
    }

    Ok(encoders)
}

#[cfg(target_os = "linux")]
fn detect_linux_vaapi() -> anyhow::Result<Vec<HwEncoder>> {
    use std::path::Path;
    use std::process::Command;

    log::info!("[encoder] L2: probing Linux VAAPI...");

    let mut encoders = Vec::new();

    let has_render_device = check_dri_render_device();

    if !has_render_device {
        log::info!("[encoder] L2: no /dev/dri/renderD* devices found, VAAPI may not be available");
    }

    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.contains("h264_vaapi") {
            let device = detect_vaapi_device_name();
            log::info!(
                "[encoder] L2: found h264_vaapi (VAAPI){}",
                device
                    .as_ref()
                    .map(|d| format!(" - {}", d))
                    .unwrap_or_default()
            );
            encoders.push(HwEncoder {
                codec: VideoCodec::H264,
                backend: EncoderBackend::Vaapi,
                device,
                priority: 2,
                ffmpeg_name: "h264_vaapi".to_string(),
            });
        }

        if stdout.contains("hevc_vaapi") {
            let device = detect_vaapi_device_name();
            log::info!(
                "[encoder] L2: found hevc_vaapi (VAAPI){}",
                device
                    .as_ref()
                    .map(|d| format!(" - {}", d))
                    .unwrap_or_default()
            );
            encoders.push(HwEncoder {
                codec: VideoCodec::Hevc,
                backend: EncoderBackend::Vaapi,
                device,
                priority: 2,
                ffmpeg_name: "hevc_vaapi".to_string(),
            });
        }

        if stdout.contains("vp9_vaapi") {
            let device = detect_vaapi_device_name();
            log::info!(
                "[encoder] L2: found vp9_vaapi (VAAPI){}",
                device
                    .as_ref()
                    .map(|d| format!(" - {}", d))
                    .unwrap_or_default()
            );
            encoders.push(HwEncoder {
                codec: VideoCodec::Vp9,
                backend: EncoderBackend::Vaapi,
                device,
                priority: 2,
                ffmpeg_name: "vp9_vaapi".to_string(),
            });
        }

        if stdout.contains("av1_vaapi") {
            let device = detect_vaapi_device_name();
            log::info!(
                "[encoder] L2: found av1_vaapi (VAAPI){}",
                device
                    .as_ref()
                    .map(|d| format!(" - {}", d))
                    .unwrap_or_default()
            );
            encoders.push(HwEncoder {
                codec: VideoCodec::Av1,
                backend: EncoderBackend::Vaapi,
                device,
                priority: 2,
                ffmpeg_name: "av1_vaapi".to_string(),
            });
        }
    }

    if encoders.is_empty() && has_render_device {
        log::info!("[encoder] L2: trying direct libva query...");
        if let Ok(va_encoders) = detect_vaapi_direct() {
            encoders.extend(va_encoders);
        }
    }

    if encoders.is_empty() {
        log::info!("[encoder] L2: no VAAPI encoders found");
    } else {
        log::info!("[encoder] L2: VAAPI found {} encoders", encoders.len());
    }

    Ok(encoders)
}

#[cfg(target_os = "linux")]
fn check_dri_render_device() -> bool {
    let render_path = Path::new("/dev/dri");

    if !render_path.exists() {
        return false;
    }

    if let Ok(entries) = std::fs::read_dir(render_path) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("renderD") {
                log::info!("[encoder] L2: found VAAPI device: {:?}", entry.path());
                return true;
            }
        }
    }

    false
}

#[cfg(target_os = "linux")]
fn detect_vaapi_device_name() -> Option<String> {
    let sys_drm = Path::new("/sys/class/drm");

    if let Ok(entries) = std::fs::read_dir(sys_drm) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name()?.to_string_lossy();

            if name.starts_with("card") {
                let driver_path = path.join("device/driver");
                if let Ok(target) = std::fs::read_link(&driver_path) {
                    let driver_name = target.file_name()?.to_string_lossy().to_string();
                    if [
                        "i915",
                        "amdgpu",
                        "nvidia",
                        "radeon",
                        "virtio_gpu",
                        " nouveau",
                    ]
                    .iter()
                    .any(|d| driver_name.contains(d))
                    {
                        let device_path = path.join("device/device");
                        if let Ok(device) = std::fs::read_to_string(device_path) {
                            let device = device.trim();
                            if !device.is_empty() && device.len() < 64 {
                                return Some(format!("{} (VAAPI)", driver_name));
                            }
                        }
                        return Some(format!("{} (VAAPI)", driver_name));
                    }
                }
            }
        }
    }

    Some("VAAPI".to_string())
}

#[cfg(target_os = "linux")]
fn detect_vaapi_direct() -> anyhow::Result<Vec<HwEncoder>> {
    use std::process::Command;

    let mut encoders = Vec::new();

    let output = Command::new("vainfo").output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let combined = format!("{}{}", stdout, stderr);

        if combined.contains("VAEntrypointEncSlice") || combined.contains("VAEntrypointEncSliceLC")
        {
            if combined.contains("H264") || combined.contains("AVC") {
                encoders.push(HwEncoder {
                    codec: VideoCodec::H264,
                    backend: EncoderBackend::Vaapi,
                    device: detect_vaapi_device_name(),
                    priority: 2,
                    ffmpeg_name: "h264_vaapi".to_string(),
                });
            }

            if combined.contains("HEVC") || combined.contains("H265") {
                encoders.push(HwEncoder {
                    codec: VideoCodec::Hevc,
                    backend: EncoderBackend::Vaapi,
                    device: detect_vaapi_device_name(),
                    priority: 2,
                    ffmpeg_name: "hevc_vaapi".to_string(),
                });
            }

            if combined.contains("VP9") {
                encoders.push(HwEncoder {
                    codec: VideoCodec::Vp9,
                    backend: EncoderBackend::Vaapi,
                    device: detect_vaapi_device_name(),
                    priority: 2,
                    ffmpeg_name: "vp9_vaapi".to_string(),
                });
            }

            if combined.contains("AV1") {
                encoders.push(HwEncoder {
                    codec: VideoCodec::Av1,
                    backend: EncoderBackend::Vaapi,
                    device: detect_vaapi_device_name(),
                    priority: 2,
                    ffmpeg_name: "av1_vaapi".to_string(),
                });
            }
        }
    }

    Ok(encoders)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_os_framework() {
        let result = detect_os_framework_encoders();
        assert!(result.is_ok());
    }
}
