//! L4 Runtime validation for hardware encoders.
//!
//! This module validates that encoders actually work at runtime by running
//! a minimal ffmpeg test with the encoder.

use std::process::Stdio;
use crate::encoder::HwEncoder;
use std::time::Instant;
use tokio::time::{timeout, Duration};

/// Timeout for each encoder validation (10 seconds).
const VALIDATION_TIMEOUT_SECS: u64 = 10;

/// Global flag to enable/disable L4 runtime validation.
/// Default is disabled - only run when explicitly requested.
static L4_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Enable or disable L4 runtime validation.
///
/// L4 validation runs actual ffmpeg commands to verify encoders work at runtime.
/// This is disabled by default to avoid startup delays and potential hangs.
pub fn set_l4_enabled(enabled: bool) {
    L4_ENABLED.store(enabled, std::sync::atomic::Ordering::SeqCst);
    log::info!(
        "[encoder] L4 runtime validation {}",
        if enabled { "enabled" } else { "disabled" }
    );
}

/// Check if L4 runtime validation is enabled.
pub fn is_l4_enabled() -> bool {
    L4_ENABLED.load(std::sync::atomic::Ordering::SeqCst)
}

/// Validate that a hardware encoder works at runtime.
///
/// This runs a minimal ffmpeg test:
/// `ffmpeg -f lavfi -i testsrc -t 1 -c:v {encoder} -f null -`
///
/// Returns `true` if the encoder runs successfully, `false` otherwise.
/// The test has a 10-second timeout per encoder.
pub async fn validate_encoder_runtime(encoder: &HwEncoder) -> bool {
    let encoder_name = &encoder.ffmpeg_name;
    log::info!("[encoder] L4: validating {}...", encoder_name);

    let start = Instant::now();

    let validation_result = timeout(
        Duration::from_secs(VALIDATION_TIMEOUT_SECS),
        run_ffmpeg_validation(encoder_name)
    ).await;

    let elapsed = start.elapsed();
    log::debug!(
        "[encoder] L4: {} validation took {:?}",
        encoder_name,
        elapsed
    );

    match validation_result {
        Ok(Ok(output)) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                log::warn!(
                    "[encoder] L4: {} validation failed with exit code {:?}: {}",
                    encoder_name,
                    output.status.code(),
                    stderr.lines().next().unwrap_or("unknown error")
                );
                return false;
            }

            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_lower = stderr.to_lowercase();
            if stderr_lower.contains("error")
                || stderr_lower.contains("failed")
                || stderr_lower.contains("cannot initialize")
                || stderr_lower.contains("not found")
                || stderr_lower.contains("unsupported")
            {
                let has_fatal_error = stderr.lines().any(|line| {
                    let lower = line.to_lowercase();
                    (lower.contains("error") || lower.contains("failed"))
                        && !lower.contains("warning")
                        && !lower.contains("deprecated")
                });

                if has_fatal_error {
                    log::warn!(
                        "[encoder] L4: {} validation detected errors: {}",
                        encoder_name,
                        stderr.lines().next().unwrap_or("unknown error")
                    );
                    return false;
                }
            }

            log::info!("[encoder] L4: {} validation succeeded", encoder_name);
            true
        }
        Ok(Err(e)) => {
            log::warn!("[encoder] L4: {} validation failed to run: {}", encoder_name, e);
            false
        }
        Err(_) => {
            log::warn!(
                "[encoder] L4: {} validation timed out after {}s",
                encoder_name,
                VALIDATION_TIMEOUT_SECS
            );
            false
        }
    }
}

async fn run_ffmpeg_validation(encoder_name: &str) -> std::io::Result<std::process::Output> {
    tokio::process::Command::new("ffmpeg")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("testsrc")
        .arg("-t")
        .arg("1")
        .arg("-c:v")
        .arg(encoder_name)
        .arg("-f")
        .arg("null")
        .arg("-")
        .arg("-nostdin")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
}

/// Validate multiple encoders with L4 runtime tests.
///
/// This runs validation on each encoder in the list and returns only
/// those that pass the runtime test. Encoders that fail are logged
/// but don't cause the entire operation to fail.
pub async fn validate_encoders_runtime(encoders: &[HwEncoder]) -> Vec<HwEncoder> {
    if !is_l4_enabled() {
        log::debug!("[encoder] L4: runtime validation disabled, skipping");
        return encoders.to_vec();
    }

    log::info!(
        "[encoder] L4: starting runtime validation for {} encoders",
        encoders.len()
    );

    let start = Instant::now();
    let mut valid_encoders = Vec::new();

    for encoder in encoders {
        if validate_encoder_runtime(encoder).await {
            valid_encoders.push(encoder.clone());
        }
    }

    let elapsed = start.elapsed();
    log::info!(
        "[encoder] L4: runtime validation complete: {}/{} valid in {:?}",
        valid_encoders.len(),
        encoders.len(),
        elapsed
    );

    if valid_encoders.is_empty() {
        log::warn!("[encoder] L4: all encoders failed validation, using fallback");
        return encoders.to_vec();
    }

    valid_encoders
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l4_enabled_by_default() {
        assert!(!is_l4_enabled());
    }

    #[test]
    fn test_set_l4_enabled() {
        set_l4_enabled(true);
        assert!(is_l4_enabled());

        set_l4_enabled(false);
        assert!(!is_l4_enabled());
    }
}
