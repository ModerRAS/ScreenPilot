use anyhow::{Context, Result};
use axum::{Router, routing::get_service};
use log::info;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

/// Pick the first non-loopback IPv4 address of the host.
///
/// Tries multiple methods to find a valid LAN IP:
/// 1. UDP connect trick to Google DNS (8.8.8.8)
/// 2. UDP connect trick to Cloudflare DNS (1.1.1.1)
/// 3. UDP connect trick to OpenDNS (208.67.222.222)
/// 4. On Windows: parse ipconfig output directly
///
/// Returns None only if no valid LAN interface is found.
pub fn local_ip() -> Option<String> {
    // Try multiple DNS servers to find a working route
    let dns_servers = [
        ("8.8.8.8", 53),
        ("1.1.1.1", 53),
        ("208.67.222.222", 443),
        ("9.9.9.9", 53),
        ("1.0.0.1", 53),
    ];

    for (ip, port) in dns_servers {
        let addr = format!("{}:{}", ip, port);
        if let Some(local) = try_local_ip(&addr) {
            return Some(local);
        }
    }

    // Fallback for Windows: parse ipconfig directly
    #[cfg(windows)]
    {
        if let Some(ip) = get_windows_lan_ip() {
            return Some(ip);
        }
    }

    None
}

fn try_local_ip(target: &str) -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    if socket.connect(target).is_ok() {
        if let Some(IpAddr::V4(ip)) = socket.local_addr().ok().map(|a| a.ip()) {
            // Reject loopback and undefined addresses
            if !ip.is_loopback() && !ip.is_unspecified() {
                return Some(ip.to_string());
            }
        }
    }
    None
}

#[cfg(windows)]
fn get_windows_lan_ip() -> Option<String> {
    use std::process::Command;

    let output = Command::new("ipconfig").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Look for lines like "IPv4 Address. . . . . . . . . . : 192.168.1.100"
    for line in stdout.lines() {
        let line = line.trim();
        if line.contains("IPv4") && line.contains(":") {
            if let Some(ip_part) = line.split(':').nth(1) {
                let ip = ip_part.trim();
                if let Ok(parsed) = ip.parse::<Ipv4Addr>() {
                    // Filter out loopback (127.x.x.x) and undefined (0.0.0.0)
                    if !parsed.is_loopback() && !parsed.is_unspecified() {
                        return Some(ip.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Start the media HTTP server and return `(actual_port, base_url)`.
///
/// The server serves all files under `media_dir` at `/media/<filename>`.
/// It binds to `0.0.0.0:port`; callers should use `local_ip()` to build
/// the public URL.
pub async fn start_media_server(
    media_dir: PathBuf,
    preferred_port: u16,
) -> Result<(u16, String)> {
    // Try the preferred port first; fall back to any available port.
    let listener = TcpListener::bind(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        preferred_port,
    ))
    .or_else(|_| {
        TcpListener::bind(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            0,
        ))
    })
    .context("bind media server TCP listener")?;

    let port = listener.local_addr()?.port();
    let ip = local_ip().unwrap_or_else(|| "127.0.0.1".to_string());
    let base_url = format!("http://{}:{}", ip, port);

    info!("Media server listening on {}/media/", base_url);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .nest_service("/media", get_service(ServeDir::new(&media_dir)))
        .layer(cors);

    // Convert std listener to tokio listener
    listener.set_nonblocking(true).context("set_nonblocking")?;
    let tokio_listener = tokio::net::TcpListener::from_std(listener)
        .context("convert to tokio TcpListener")?;

    tokio::spawn(async move {
        if let Err(e) = axum::serve(tokio_listener, app).await {
            log::error!("Media server error: {e}");
        }
    });

    Ok((port, base_url))
}

/// List the filenames (not paths) available in `media_dir`.
pub fn list_media_files(media_dir: &PathBuf) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(media_dir) else {
        return vec![];
    };
    let mut files: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_list_media_files_empty_dir() {
        let tmp = std::env::temp_dir().join("sp_test_empty");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let files = list_media_files(&tmp);
        assert!(files.is_empty());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_list_media_files_with_files() {
        let tmp = std::env::temp_dir().join("sp_test_media");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("ad.mp4"), b"").unwrap();
        fs::write(tmp.join("promo.mp4"), b"").unwrap();
        fs::write(tmp.join("menu.mp4"), b"").unwrap();

        let files = list_media_files(&tmp);
        assert_eq!(files, vec!["ad.mp4", "menu.mp4", "promo.mp4"]);
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_list_media_files_nonexistent() {
        let path = PathBuf::from("/nonexistent/path/for/test");
        let files = list_media_files(&path);
        assert!(files.is_empty());
    }

    #[tokio::test]
    async fn test_media_server_starts() {
        let tmp = std::env::temp_dir().join("sp_test_server");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("test.mp4"), b"fake video content").unwrap();

        let (port, base_url) = start_media_server(tmp.clone(), 0).await.unwrap();
        assert!(port > 0);
        assert!(base_url.contains(':'));

        // Give the server a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Try to fetch the file
        let url = format!("{}/media/test.mp4", base_url);
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 200);
        let body = resp.bytes().await.unwrap();
        assert_eq!(body.as_ref(), b"fake video content");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
