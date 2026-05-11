use anyhow::{Context, Result};
use axum::{routing::get_service, Router};
use log::info;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

/// Pick the first non-loopback IPv4 address of the host.
pub fn local_ip() -> Option<String> {
    local_ipv4_candidates()
        .into_iter()
        .next()
        .map(|ip| ip.to_string())
}

/// Return LAN IPv4 candidates in preference order.
///
/// SSDP multicast can fail on multi-adapter machines if the wrong outgoing
/// interface is selected, so callers can use this list to choose a LAN route.
pub fn local_ipv4_candidates() -> Vec<Ipv4Addr> {
    let mut candidates = Vec::new();

    // Keep the old successful probe first for compatibility with environments
    // where DNS ports are filtered but normal web routing is available.
    let route_probes = [
        "8.8.8.8:80",
        "1.1.1.1:80",
        "8.8.8.8:53",
        "1.1.1.1:53",
        "208.67.222.222:443",
        "9.9.9.9:53",
    ];

    for target in route_probes {
        if let Some(local) = try_local_ip(target) {
            push_unique_ip(&mut candidates, local);
        }
    }

    #[cfg(windows)]
    {
        for ip in get_windows_lan_ips() {
            push_unique_ip(&mut candidates, ip);
        }
    }

    candidates
}

fn try_local_ip(target: &str) -> Option<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    if socket.connect(target).is_ok() {
        if let Some(IpAddr::V4(ip)) = socket.local_addr().ok().map(|a| a.ip()) {
            if is_usable_lan_ip(ip) {
                return Some(ip);
            }
        }
    }
    None
}

fn push_unique_ip(ips: &mut Vec<Ipv4Addr>, ip: Ipv4Addr) {
    if is_usable_lan_ip(ip) && !ips.contains(&ip) {
        ips.push(ip);
    }
}

fn is_usable_lan_ip(ip: Ipv4Addr) -> bool {
    !ip.is_loopback() && !ip.is_unspecified() && !ip.is_multicast() && !ip.is_broadcast()
}

#[cfg(windows)]
fn get_windows_lan_ips() -> Vec<Ipv4Addr> {
    use std::process::Command;

    let Some(output) = Command::new("ipconfig").output().ok() else {
        return vec![];
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut ips = Vec::new();

    // English and localized Windows output both keep "IPv4" in these lines.
    for line in stdout.lines() {
        let line = line.trim();
        if line.contains("IPv4") {
            if let Some(ip_part) = line.rsplit(':').next() {
                let ip = ip_part.trim();
                if let Ok(parsed) = ip.parse::<Ipv4Addr>() {
                    push_unique_ip(&mut ips, parsed);
                }
            }
        }
    }

    ips
}

/// Start the media HTTP server and return `(actual_port, base_url)`.
///
/// The server serves all files under `media_dir` at `/media/<filename>`.
/// It binds to `0.0.0.0:port`; callers should use `local_ip()` to build
/// the public URL.
pub async fn start_media_server(media_dir: PathBuf, preferred_port: u16) -> Result<(u16, String)> {
    // Try the preferred port first; fall back to any available port.
    let listener = TcpListener::bind(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        preferred_port,
    ))
    .or_else(|_| TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)))
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
    let tokio_listener =
        tokio::net::TcpListener::from_std(listener).context("convert to tokio TcpListener")?;

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
        let url = format!("http://127.0.0.1:{}/media/test.mp4", port);
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let resp = client.get(&url).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let body = resp.bytes().await.unwrap();
        assert_eq!(body.as_ref(), b"fake video content");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
