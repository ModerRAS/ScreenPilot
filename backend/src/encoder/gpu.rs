//! GPU native detection for hardware video encoders.
//!
//! This module detects hardware encoders via GPU vendor APIs:
//! - NVIDIA: NVENC (h264_nvenc, hevc_nvenc, av1_nvenc)
//! - Intel: QSV (h264_qsv, hevc_qsv)
//! - AMD: AMF (h264_amf, hevc_amf)

use super::{EncoderBackend, HwEncoder, VideoCodec};

/// Detect hardware encoders via GPU vendor APIs (L1).
///
/// Returns a list of detected hardware encoders with priority=1 (highest).
pub fn detect_gpu_encoders() -> anyhow::Result<Vec<HwEncoder>> {
    let mut encoders = Vec::new();

    log::info!("[encoder] L1: probing GPU vendor APIs...");

    // Detect NVIDIA
    if let Some(nvidia_encoders) = detect_nvidia() {
        log::info!(
            "[encoder] L1: NVIDIA GPU detected, adding {} encoders",
            nvidia_encoders.len()
        );
        encoders.extend(nvidia_encoders);
    }

    // Detect Intel
    if let Some(intel_encoders) = detect_intel() {
        log::info!(
            "[encoder] L1: Intel GPU detected, adding {} encoders",
            intel_encoders.len()
        );
        encoders.extend(intel_encoders);
    }

    // Detect AMD
    if let Some(amd_encoders) = detect_amd() {
        log::info!(
            "[encoder] L1: AMD GPU detected, adding {} encoders",
            amd_encoders.len()
        );
        encoders.extend(amd_encoders);
    }

    if encoders.is_empty() {
        log::info!("[encoder] L1: no GPU hardware encoders detected");
    }

    Ok(encoders)
}

/// Detect NVIDIA GPU and return available encoders.
fn detect_nvidia() -> Option<Vec<HwEncoder>> {
    let has_nvidia = match std::env::consts::OS {
        "windows" => check_nvidia_windows(),
        "linux" => check_nvidia_linux(),
        _ => false,
    };

    if has_nvidia {
        Some(vec![
            HwEncoder {
                codec: VideoCodec::H264,
                backend: EncoderBackend::Nvenc,
                device: get_nvidia_device_name(),
                priority: 1,
                ffmpeg_name: "h264_nvenc".to_string(),
            },
            HwEncoder {
                codec: VideoCodec::Hevc,
                backend: EncoderBackend::Nvenc,
                device: get_nvidia_device_name(),
                priority: 1,
                ffmpeg_name: "hevc_nvenc".to_string(),
            },
            HwEncoder {
                codec: VideoCodec::Av1,
                backend: EncoderBackend::Nvenc,
                device: get_nvidia_device_name(),
                priority: 1,
                ffmpeg_name: "av1_nvenc".to_string(),
            },
        ])
    } else {
        None
    }
}

/// Check for NVIDIA GPU on Windows via registry.
#[cfg(target_os = "windows")]
fn check_nvidia_windows() -> bool {
    use std::process::Command;

    // Check registry for NVIDIA GPU
    let output = Command::new("reg")
        .args([
            "query",
            r"HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}",
            "/v",
            "DriverDesc",
        ])
        .output();

    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            stdout.contains("NVIDIA") || stdout.contains("nvidia")
        }
        Err(_) => false,
    }
}

#[cfg(not(target_os = "windows"))]
fn check_nvidia_windows() -> bool {
    false
}

/// Check for NVIDIA GPU on Linux via /proc/driver/nvidia or nvidia-smi.
#[cfg(target_os = "linux")]
fn check_nvidia_linux() -> bool {
    use std::path::Path;

    // Check /proc/driver/nvidia/version
    if Path::new("/proc/driver/nvidia/version").exists() {
        return true;
    }

    // Check for nvidia-smi
    std::process::Command::new("nvidia-smi")
        .arg("--query-gpu=name")
        .arg("--format=csv,noheader")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn check_nvidia_linux() -> bool {
    false
}

/// Get NVIDIA GPU device name.
fn get_nvidia_device_name() -> Option<String> {
    match std::env::consts::OS {
        "windows" => {
            // Try using nvidia-smi on Windows as well
            get_nvidia_smi_name()
        }
        "linux" => get_nvidia_smi_name(),
        _ => None,
    }
}

/// Get NVIDIA device name via nvidia-smi.
fn get_nvidia_smi_name() -> Option<String> {
    std::process::Command::new("nvidia-smi")
        .arg("--query-gpu=name")
        .arg("--format=csv,noheader")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let name = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if name.is_empty() {
                None
            } else {
                Some(name)
            }
        })
}

/// Detect Intel GPU and return available encoders.
fn detect_intel() -> Option<Vec<HwEncoder>> {
    let has_intel = match std::env::consts::OS {
        "windows" => check_intel_windows(),
        "linux" => check_intel_linux(),
        _ => false,
    };

    if has_intel {
        Some(vec![
            HwEncoder {
                codec: VideoCodec::H264,
                backend: EncoderBackend::Qsv,
                device: get_intel_device_name(),
                priority: 1,
                ffmpeg_name: "h264_qsv".to_string(),
            },
            HwEncoder {
                codec: VideoCodec::Hevc,
                backend: EncoderBackend::Qsv,
                device: get_intel_device_name(),
                priority: 1,
                ffmpeg_name: "hevc_qsv".to_string(),
            },
        ])
    } else {
        None
    }
}

/// Check for Intel GPU on Windows via registry.
#[cfg(target_os = "windows")]
fn check_intel_windows() -> bool {
    use std::process::Command;

    let output = Command::new("reg")
        .args([
            "query",
            r"HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}",
            "/v",
            "DriverDesc",
        ])
        .output();

    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            stdout.contains("Intel") || stdout.contains("intel")
        }
        Err(_) => false,
    }
}

#[cfg(not(target_os = "windows"))]
fn check_intel_windows() -> bool {
    false
}

/// Check for Intel GPU on Linux via /sys/class/drm or lspci.
#[cfg(target_os = "linux")]
fn check_intel_linux() -> bool {
    use std::path::Path;

    // Check /sys/class/drm for Intel GPUs
    if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                if name.starts_with("card") && !name.contains("-") {
                    let device_path = entry.path().join("device");
                    if device_path.exists() {
                        if let Ok(vendor) = std::fs::read_to_string(device_path.join("vendor")) {
                            // Intel vendor ID is 0x8086
                            if vendor.trim().contains("0x8086") {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback: check lspci
    std::process::Command::new("lspci")
        .arg("-n")
        .output()
        .ok()
        .map(|o| {
            let output = String::from_utf8_lossy(&o.stdout);
            output.contains("8086") && (output.contains("VGA") || output.contains("Display"))
        })
        .unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn check_intel_linux() -> bool {
    false
}

/// Get Intel GPU device name.
fn get_intel_device_name() -> Option<String> {
    match std::env::consts::OS {
        "windows" => {
            // On Windows, return a generic name since there's no easy way to get the exact model
            Some("Intel GPU".to_string())
        }
        "linux" => {
            // Try to get from lspci
            std::process::Command::new("lspci")
                .args(["-v", "-s", "00:02.0"])
                .output()
                .ok()
                .and_then(|o| {
                    let output = String::from_utf8_lossy(&o.stdout);
                    output
                        .lines()
                        .find(|l| l.contains("VGA") || l.contains("Display"))
                        .map(|l| {
                            // Extract device name from lspci output
                            l.split(':')
                                .nth(2)
                                .map(|s| s.trim().to_string())
                                .unwrap_or_else(|| "Intel GPU".to_string())
                        })
                })
                .or_else(|| Some("Intel GPU".to_string()))
        }
        _ => None,
    }
}

/// Detect AMD GPU and return available encoders.
fn detect_amd() -> Option<Vec<HwEncoder>> {
    let has_amd = match std::env::consts::OS {
        "windows" => check_amd_windows(),
        "linux" => check_amd_linux(),
        _ => false,
    };

    if has_amd {
        Some(vec![
            HwEncoder {
                codec: VideoCodec::H264,
                backend: EncoderBackend::Amf,
                device: get_amd_device_name(),
                priority: 1,
                ffmpeg_name: "h264_amf".to_string(),
            },
            HwEncoder {
                codec: VideoCodec::Hevc,
                backend: EncoderBackend::Amf,
                device: get_amd_device_name(),
                priority: 1,
                ffmpeg_name: "hevc_amf".to_string(),
            },
        ])
    } else {
        None
    }
}

/// Check for AMD GPU on Windows via registry.
#[cfg(target_os = "windows")]
fn check_amd_windows() -> bool {
    use std::process::Command;

    let output = Command::new("reg")
        .args([
            "query",
            r"HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}",
            "/v",
            "DriverDesc",
        ])
        .output();

    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            stdout.contains("AMD") || stdout.contains("Radeon")
        }
        Err(_) => false,
    }
}

#[cfg(not(target_os = "windows"))]
fn check_amd_windows() -> bool {
    false
}

/// Check for AMD GPU on Linux via /sys/class/drm or lspci.
#[cfg(target_os = "linux")]
fn check_amd_linux() -> bool {
    // Check lspci for AMD GPUs
    std::process::Command::new("lspci")
        .arg("-n")
        .output()
        .ok()
        .map(|o| {
            let output = String::from_utf8_lossy(&o.stdout);
            // AMD vendor IDs: 0x1002 (old), 0x1022 (new)
            (output.contains("1002") || output.contains("1022"))
                && (output.contains("VGA") || output.contains("Display"))
        })
        .unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn check_amd_linux() -> bool {
    false
}

/// Get AMD GPU device name.
fn get_amd_device_name() -> Option<String> {
    match std::env::consts::OS {
        "windows" => {
            // On Windows, return a generic name
            Some("AMD GPU".to_string())
        }
        "linux" => {
            // Try to get from lspci
            std::process::Command::new("lspci")
                .arg("-v")
                .output()
                .ok()
                .and_then(|o| {
                    let output = String::from_utf8_lossy(&o.stdout);
                    output
                        .lines()
                        .find(|l| {
                            (l.contains("AMD") || l.contains("Radeon"))
                                && (l.contains("VGA") || l.contains("Display"))
                        })
                        .map(|l| {
                            l.split(':')
                                .nth(2)
                                .map(|s| s.trim().to_string())
                                .unwrap_or_else(|| "AMD GPU".to_string())
                        })
                })
                .or_else(|| Some("AMD GPU".to_string()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_gpu_encoders() {
        let result = detect_gpu_encoders();
        assert!(result.is_ok());
        // May or may not have encoders depending on the test environment
        let encoders = result.unwrap();
        log::info!("Detected {} GPU encoders", encoders.len());
    }
}
