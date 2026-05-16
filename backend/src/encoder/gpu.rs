//! GPU native detection for hardware video encoders.
//!
//! This module detects hardware encoders via GPU vendor APIs:
//! - NVIDIA: NVENC (h264_nvenc, hevc_nvenc, av1_nvenc)
//! - Intel: QSV (h264_qsv, hevc_qsv)
//! - AMD: AMF (h264_amf, hevc_amf)

use super::{EncoderBackend, HwEncoder, VideoCodec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    Nvidia,
    Intel,
    Amd,
}

pub fn detect_gpu_vendors() -> Vec<GpuVendor> {
    let mut vendors = Vec::new();

    if match std::env::consts::OS {
        "windows" => check_nvidia_windows(),
        "linux" => check_nvidia_linux(),
        _ => false,
    } {
        vendors.push(GpuVendor::Nvidia);
    }

    if match std::env::consts::OS {
        "windows" => check_intel_windows(),
        "linux" => check_intel_linux(),
        _ => false,
    } {
        vendors.push(GpuVendor::Intel);
    }

    if match std::env::consts::OS {
        "windows" => check_amd_windows(),
        "linux" => check_amd_linux(),
        _ => false,
    } {
        vendors.push(GpuVendor::Amd);
    }

    vendors
}

pub fn backend_matches_detected_gpu(backend: EncoderBackend, vendors: &[GpuVendor]) -> bool {
    match backend {
        EncoderBackend::Nvenc => vendors.contains(&GpuVendor::Nvidia),
        EncoderBackend::Qsv => vendors.contains(&GpuVendor::Intel),
        EncoderBackend::Amf => vendors.contains(&GpuVendor::Amd),
        EncoderBackend::Vaapi => {
            vendors.contains(&GpuVendor::Intel)
                || vendors.contains(&GpuVendor::Amd)
                || vendors.contains(&GpuVendor::Nvidia)
        }
        EncoderBackend::Videotoolbox => cfg!(target_os = "macos"),
        EncoderBackend::Mf => cfg!(target_os = "windows"),
        EncoderBackend::Software => true,
    }
}

/// Detect hardware encoders via GPU vendor APIs (L1).
///
/// Returns a list of detected hardware encoders with priority=1 (highest).
pub fn detect_gpu_encoders() -> anyhow::Result<Vec<HwEncoder>> {
    let mut encoders = Vec::new();

    log::info!("[encoder] L1: probing GPU vendor APIs...");

    let vendors = detect_gpu_vendors();

    if vendors.contains(&GpuVendor::Nvidia) {
        let nvidia_encoders = nvidia_encoders();
        log::info!(
            "[encoder] L1: NVIDIA GPU detected, adding {} encoders",
            nvidia_encoders.len()
        );
        encoders.extend(nvidia_encoders);
    }

    if vendors.contains(&GpuVendor::Intel) {
        let intel_encoders = intel_encoders();
        log::info!(
            "[encoder] L1: Intel GPU detected, adding {} encoders",
            intel_encoders.len()
        );
        encoders.extend(intel_encoders);
    }

    if vendors.contains(&GpuVendor::Amd) {
        let amd_encoders = amd_encoders();
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

fn nvidia_encoders() -> Vec<HwEncoder> {
    vec![
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
    ]
}

/// Check for NVIDIA GPU on Windows via registry.
#[cfg(target_os = "windows")]
fn check_nvidia_windows() -> bool {
    windows_gpu_name_matches(&["nvidia"])
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

fn intel_encoders() -> Vec<HwEncoder> {
    vec![
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
    ]
}

/// Check for Intel GPU on Windows via registry.
#[cfg(target_os = "windows")]
fn check_intel_windows() -> bool {
    windows_gpu_name_matches(&["intel"])
}

#[cfg(not(target_os = "windows"))]
fn check_intel_windows() -> bool {
    false
}

/// Check for Intel GPU on Linux via /sys/class/drm or lspci.
#[cfg(target_os = "linux")]
fn check_intel_linux() -> bool {
    use std::path::Path;

    if drm_vendor_exists(Path::new("/sys/class/drm"), "0x8086") {
        return true;
    }

    // Fallback: check lspci
    std::process::Command::new("lspci")
        .arg("-n")
        .output()
        .ok()
        .map(|o| {
            let output = String::from_utf8_lossy(&o.stdout);
            lspci_vendor_has_display(&output, "8086")
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

fn amd_encoders() -> Vec<HwEncoder> {
    vec![
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
    ]
}

/// Check for AMD GPU on Windows via registry.
#[cfg(target_os = "windows")]
fn check_amd_windows() -> bool {
    windows_gpu_name_matches(&["amd", "radeon"])
}

#[cfg(not(target_os = "windows"))]
fn check_amd_windows() -> bool {
    false
}

/// Check for AMD GPU on Linux via /sys/class/drm or lspci.
#[cfg(target_os = "linux")]
fn check_amd_linux() -> bool {
    if drm_vendor_exists(std::path::Path::new("/sys/class/drm"), "0x1002") {
        return true;
    }

    std::process::Command::new("lspci")
        .arg("-n")
        .output()
        .ok()
        .map(|o| {
            let output = String::from_utf8_lossy(&o.stdout);
            lspci_vendor_has_display(&output, "1002")
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

#[cfg(target_os = "windows")]
fn windows_gpu_name_matches(needles: &[&str]) -> bool {
    use std::process::Command;

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name }",
        ])
        .output();

    output
        .ok()
        .filter(|result| result.status.success())
        .map(|result| {
            let stdout = String::from_utf8_lossy(&result.stdout).to_lowercase();
            needles.iter().any(|needle| stdout.contains(needle))
        })
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn drm_vendor_exists(drm_root: &std::path::Path, vendor_id: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(drm_root) else {
        return false;
    };

    entries.flatten().any(|entry| {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("card") || name.contains('-') {
            return false;
        }

        std::fs::read_to_string(entry.path().join("device/vendor"))
            .map(|vendor| vendor.trim().eq_ignore_ascii_case(vendor_id))
            .unwrap_or(false)
    })
}

fn lspci_vendor_has_display(output: &str, vendor_id: &str) -> bool {
    let vendor = vendor_id.trim_start_matches("0x").to_ascii_lowercase();

    output.lines().any(|line| {
        let lower = line.to_ascii_lowercase();
        let is_display =
            lower.contains("vga") || lower.contains("display") || lower.contains("3d controller");
        is_display && lower.contains(&vendor)
    })
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

    #[test]
    fn test_lspci_vendor_has_display_checks_same_line() {
        let output = "\
00:02.0 VGA compatible controller [0300]: Intel Corporation Device [8086:a788]
00:14.0 USB controller [0c03]: Advanced Micro Devices, Inc. [AMD] Device [1022:abcd]
";

        assert!(lspci_vendor_has_display(output, "8086"));
        assert!(!lspci_vendor_has_display(output, "1002"));
    }

    #[test]
    fn test_backend_matches_detected_gpu() {
        let vendors = vec![GpuVendor::Intel];

        assert!(backend_matches_detected_gpu(EncoderBackend::Qsv, &vendors));
        assert!(!backend_matches_detected_gpu(EncoderBackend::Amf, &vendors));
        assert!(!backend_matches_detected_gpu(
            EncoderBackend::Nvenc,
            &vendors
        ));
    }
}
