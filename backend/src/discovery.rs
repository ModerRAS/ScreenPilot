use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;
use std::io::Read;

use anyhow::{Context, Result};
use log::{debug, info, warn};
use regex::Regex;
use reqwest::Client;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::time::sleep;

use crate::media_server;
use crate::state::RendererDevice;

const SSDP_ADDR: &str = "239.255.255.250";
const SSDP_PORT: u16 = 1900;
const SEARCH_TARGETS: &[&str] = &[
    "ssdp:all",
    "urn:schemas-upnp-org:device:MediaRenderer:1",
    "urn:schemas-upnp-org:device:MediaRenderer:2",
];
const MX: u8 = 5; // 5 seconds per target (total ~16s for 3 targets)

/// Send M-SEARCH multicast for each search target and collect unique location URLs.
fn ssdp_search(_timeout: Duration) -> Result<Vec<String>> {
    let mut locations: Vec<String> = Vec::new();
    let target_timeout = Duration::from_secs(MX as u64 + 1);

    for target in SEARCH_TARGETS {
        let single_locations = search_single_target(target, target_timeout)?;
        for loc in single_locations {
            if !locations.contains(&loc) {
                locations.push(loc);
            }
        }
        if *target != *SEARCH_TARGETS.last().unwrap() {
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    Ok(locations)
}

fn search_single_target(target: &str, timeout: Duration) -> Result<Vec<String>> {
    info!("Starting SSDP search for target: {}", target);
    
    let raw_socket =
        Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).context("create UDP socket")?;
    raw_socket.set_reuse_address(true).context("set_reuse_address")?;
    
    raw_socket.set_multicast_ttl_v4(4).context("set_multicast_ttl_v4")?;

    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
    raw_socket.bind(&bind_addr.into()).context("bind UDP socket")?;

    raw_socket
        .set_multicast_loop_v4(true)
        .context("set_multicast_loop_v4")?;

    let multicast_iface = media_server::local_ip()
        .and_then(|ip| ip.parse::<Ipv4Addr>().ok())
        .unwrap_or_else(|| Ipv4Addr::new(0, 0, 0, 0));

    info!("Using local IP {} for multicast interface", multicast_iface);

    raw_socket
        .join_multicast_v4(
            &"239.255.255.250".parse::<Ipv4Addr>().unwrap(),
            &multicast_iface,
        )
        .context("join_multicast_v4")?;

    let socket: UdpSocket = raw_socket.into();
    socket
        .set_read_timeout(Some(timeout))
        .context("set_read_timeout")?;

    let request = format!(
        "M-SEARCH * HTTP/1.1\r\n\
         HOST: {}:{}\r\n\
         MAN: \"ssdp:discover\"\r\n\
         MX: {}\r\n\
         ST: {}\r\n\
         \r\n",
        SSDP_ADDR, SSDP_PORT, MX, target
    );

    let dest = SocketAddr::new(
        IpAddr::V4(SSDP_ADDR.parse::<Ipv4Addr>().unwrap()),
        SSDP_PORT,
    );
    
    debug!("Sending M-SEARCH request:\n{}", request);
    
    socket
        .send_to(request.as_bytes(), dest)
        .context("send M-SEARCH")?;

    info!("M-SEARCH sent to {}:{}, waiting for responses...", SSDP_ADDR, SSDP_PORT);
    eprintln!("[DEBUG] M-SEARCH sent for target: {}", target);

    let mut locations: Vec<String> = Vec::new();
    let mut buf = [0u8; 4096];
    let mut response_count = 0;

    loop {
        match socket.recv_from(&mut buf) {
            Ok((len, src)) => {
                response_count += 1;
                eprintln!("[DEBUG] Received {} bytes from {}", len, src);
                info!("Received SSDP response #{} from {}", response_count, src);
                let response = String::from_utf8_lossy(&buf[..len]);
                eprintln!("[DEBUG] Raw response: {:?}", &response[..response.len().min(200)]);
                debug!("SSDP response:\n{}", response);
                if let Some(location) = parse_location(&response) {
                    eprintln!("[DEBUG] Found location: {}", location);
                    if !locations.contains(&location) {
                        locations.push(location);
                    }
                }
            }
            Err(e) if is_timeout_error(&e) => {
                eprintln!("[DEBUG] Timeout after {} responses", response_count);
                break;
            }
            Err(e) => {
                warn!("SSDP recv error: {e}");
                break;
            }
        }
    }

    info!("SSDP discovery complete. Received {} responses, found {} locations", response_count, locations.len());

    Ok(locations)
}

fn is_timeout_error(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    )
}

fn parse_location(response: &str) -> Option<String> {
    for line in response.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("location:") {
            return Some(line[9..].trim().to_string());
        }
    }
    None
}

/// Fetch the device description XML at `location` and extract the device info.
async fn fetch_device_description(
    client: &Client,
    location: &str,
) -> Result<RendererDevice> {
    let max_retries = 3;

    for attempt in 0..max_retries {
        match fetch_device_description_once(client, location).await {
            Ok(device) => return Ok(device),
            Err(e) if attempt < max_retries - 1 => {
                let delay_ms = 200u64 * 2u64.pow(attempt as u32);
                let delay = Duration::from_millis(delay_ms);
                tokio::time::sleep(delay).await;
                log::debug!("Retry fetch device description for {}: {}", location, e);
            }
            Err(e) => return Err(e),
        }
    }

    unreachable!()
}

async fn fetch_device_description_once(
    client: &Client,
    location: &str,
) -> Result<RendererDevice> {
    let body = client
        .get(location)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .context("GET device description")?
        .text()
        .await
        .context("read device description body")?;

    let uuid = extract_xml_text(&body, "UDN")
        .map(|u| u.trim_start_matches("uuid:").to_string())
        .unwrap_or_else(|| location.to_string());

    let name = extract_xml_text(&body, "friendlyName")
        .unwrap_or_else(|| "Unknown".to_string());

    // Parse IP from the location URL
    let ip = url_host(location).unwrap_or_else(|| "unknown".to_string());

    // Find the AVTransport controlURL
    let av_transport_url = find_av_transport_url(&body, location)
        .unwrap_or_else(|| format!("{}/upnp/control/AVTransport", base_url(location)));

    Ok(RendererDevice {
        uuid,
        name,
        ip,
        av_transport_url,
        status: crate::state::PlaybackStatus::Idle,
        current_media: None,
        loop_playback: false,
    })
}

fn extract_xml_text<'a>(xml: &'a str, tag: &str) -> Option<String> {
    // Match <tag>content</tag> or <ns:tag>content</ns:tag> where ns is any namespace prefix
    let re = Regex::new(&format!(r"<(?:\w+:)?{}>([^<]*)</(?:\w+:)?{}>", tag, tag)).ok()?;
    let caps = re.captures(xml)?;
    caps.get(1).map(|m| m.as_str().to_string())
}

/// Find the AVTransport service controlURL within the device description XML.
fn find_av_transport_url(xml: &str, location: &str) -> Option<String> {
    // First try to get URLBase if present
    let url_base = extract_xml_text(xml, "URLBase");
    
    // Locate the AVTransport serviceType, then find the next controlURL
    let service_marker = "AVTransport";
    let service_pos = xml.find(service_marker)?;
    let after = &xml[service_pos..];

    let ctrl_open = "<controlURL>";
    let ctrl_close = "</controlURL>";
    let start = after.find(ctrl_open)? + ctrl_open.len();
    let end = after[start..].find(ctrl_close)? + start;
    let path = after[start..end].trim().to_string();

    // Build absolute URL
    let base = url_base
        .or_else(|| Some(base_url(location)))
        .unwrap();
    
    if path.starts_with("http") {
        Some(path)
    } else if path.starts_with('/') {
        Some(format!("{}{}", base, path))
    } else {
        Some(format!("{}/{}", base, path))
    }
}

fn base_url(url: &str) -> String {
    // e.g. "http://192.168.1.5:49152/description.xml" -> "http://192.168.1.5:49152"
    if let Some(idx) = url[8..].find('/') {
        url[..idx + 8].to_string()
    } else {
        url.to_string()
    }
}

fn url_host(url: &str) -> Option<String> {
    // Strip scheme
    let without_scheme = if let Some(s) = url.strip_prefix("http://") {
        s
    } else if let Some(s) = url.strip_prefix("https://") {
        s
    } else {
        return None;
    };

    // Take up to the next '/'
    let host_port = without_scheme.split('/').next()?;
    // Remove port
    let host = host_port.split(':').next()?;
    Some(host.to_string())
}

/// Discover all DLNA MediaRenderer devices on the LAN.
/// Returns a deduplicated list of `RendererDevice`.
pub async fn discover_renderers() -> Vec<RendererDevice> {
    let locations = match ssdp_search(Duration::from_secs(MX as u64 + 1)) {
        Ok(l) => l,
        Err(e) => {
            warn!("SSDP search failed: {e}");
            return vec![];
        }
    };

    let client = Client::new();
    let mut devices = Vec::new();

    for loc in locations {
        match fetch_device_description(&client, &loc).await {
            Ok(d) => {
                debug!("Discovered device: {} ({})", d.name, d.uuid);
                devices.push(d);
            }
            Err(e) => {
                warn!("Failed to fetch device description from {loc}: {e}");
            }
        }
    }

    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_location() {
        let response = "HTTP/1.1 200 OK\r\nLOCATION: http://192.168.1.5:49152/desc.xml\r\nST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n";
        assert_eq!(
            parse_location(response),
            Some("http://192.168.1.5:49152/desc.xml".to_string())
        );
    }

    #[test]
    fn test_parse_location_missing() {
        let response = "HTTP/1.1 200 OK\r\nST: something\r\n\r\n";
        assert_eq!(parse_location(response), None);
    }

    #[test]
    fn test_extract_xml_text() {
        let xml = "<root><friendlyName>My TV</friendlyName><UDN>uuid:abc-123</UDN></root>";
        assert_eq!(
            extract_xml_text(xml, "friendlyName"),
            Some("My TV".to_string())
        );
        assert_eq!(
            extract_xml_text(xml, "UDN"),
            Some("uuid:abc-123".to_string())
        );
        assert_eq!(extract_xml_text(xml, "missing"), None);
    }

    #[test]
    fn test_base_url() {
        assert_eq!(
            base_url("http://192.168.1.5:49152/description.xml"),
            "http://192.168.1.5:49152"
        );
    }

    #[test]
    fn test_url_host() {
        assert_eq!(
            url_host("http://192.168.1.5:49152/description.xml"),
            Some("192.168.1.5".to_string())
        );
        assert_eq!(url_host("not-a-url"), None);
    }

    #[test]
    fn test_find_av_transport_url_relative() {
        let xml = r#"
            <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
            <controlURL>/upnp/control/AVTransport</controlURL>
        "#;
        let result = find_av_transport_url(xml, "http://192.168.1.5:49152/desc.xml");
        assert_eq!(
            result,
            Some("http://192.168.1.5:49152/upnp/control/AVTransport".to_string())
        );
    }

    #[test]
    fn test_find_av_transport_url_absolute() {
        let xml = r#"
            <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
            <controlURL>http://192.168.1.5:49152/upnp/control/AVTransport</controlURL>
        "#;
        let result = find_av_transport_url(xml, "http://192.168.1.5:49152/desc.xml");
        assert_eq!(
            result,
            Some("http://192.168.1.5:49152/upnp/control/AVTransport".to_string())
        );
    }

    #[test]
    fn test_parse_location_case_insensitive() {
        let response = "HTTP/1.1 200 OK\r\nlocation: http://192.168.1.5:49152/desc.xml\r\n\r\n";
        assert_eq!(
            parse_location(response),
            Some("http://192.168.1.5:49152/desc.xml".to_string())
        );
    }

    #[test]
    fn test_parse_location_multiple_spaces() {
        let response = "HTTP/1.1 200 OK\r\nLOCATION:   http://192.168.1.5:49152/desc.xml  \r\n\r\n";
        assert_eq!(
            parse_location(response),
            Some("http://192.168.1.5:49152/desc.xml".to_string())
        );
    }

    #[test]
    fn test_parse_location_no_location() {
        let response = "HTTP/1.1 200 OK\r\nSERVER: Test/1.0\r\n\r\n";
        assert_eq!(parse_location(response), None);
    }

    #[test]
    fn test_extract_xml_text_with_namespace() {
        let xml = r#"<root xmlns:u="urn:schemas-upnp-org"><u:friendlyName>Bedroom TV</u:friendlyName></root>"#;
        assert_eq!(extract_xml_text(xml, "friendlyName"), Some("Bedroom TV".to_string()));
    }

    #[test]
    fn test_extract_xml_text_embedded_tags() {
        let xml = "<root><friendlyName>TV<special/>Tag</friendlyName></root>";
        let result = extract_xml_text(xml, "friendlyName");
        assert!(result.is_none() || result == Some("TV<special/>Tag".to_string()));
    }

    #[test]
    fn test_base_url_no_path() {
        assert_eq!(base_url("http://192.168.1.5:49152"), "http://192.168.1.5:49152");
    }

    #[test]
    fn test_base_url_with_root() {
        assert_eq!(base_url("http://192.168.1.5:49152/"), "http://192.168.1.5:49152");
    }

    #[test]
    fn test_url_host_with_port() {
        assert_eq!(
            url_host("http://192.168.1.5:49152/desc.xml"),
            Some("192.168.1.5".to_string())
        );
    }

    #[test]
    fn test_url_host_https() {
        assert_eq!(
            url_host("https://192.168.1.5:443/desc.xml"),
            Some("192.168.1.5".to_string())
        );
    }

    #[test]
    fn test_url_host_no_scheme() {
        assert_eq!(url_host("192.168.1.5:49152/desc.xml"), None);
    }

    #[test]
    fn test_url_host_empty() {
        assert_eq!(url_host(""), None);
    }

    #[test]
    fn test_find_av_transport_url_not_found() {
        let xml = r#"<root><serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType></root>"#;
        let result = find_av_transport_url(xml, "http://192.168.1.5:49152/desc.xml");
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_av_transport_url_with_urlbase() {
        // Xiaomi Redmi TV style: controlURL starts with underscore, URLBase provided
        let xml = r#"
            <URLBase>http://192.168.1.123:49152</URLBase>
            <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
            <controlURL>_urn:schemas-upnp-org:service:AVTransport_control</controlURL>
        "#;
        let result = find_av_transport_url(xml, "http://192.168.1.5:49152/desc.xml");
        assert_eq!(
            result,
            Some("http://192.168.1.123:49152/_urn:schemas-upnp-org:service:AVTransport_control".to_string())
        );
    }

    #[test]
    fn test_find_av_transport_url_relative_path_with_underscore() {
        // Relative path starting with underscore (Xiaomi devices)
        let xml = r#"
            <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
            <controlURL>_urn:schemas-upnp-org:service:AVTransport_control</controlURL>
        "#;
        let result = find_av_transport_url(xml, "http://192.168.1.5:49152/desc.xml");
        assert_eq!(
            result,
            Some("http://192.168.1.5:49152/_urn:schemas-upnp-org:service:AVTransport_control".to_string())
        );
    }

    #[test]
    fn test_extract_xml_text_urlbase() {
        let xml = r#"<root><URLBase>http://192.168.1.123:49152</URLBase></root>"#;
        assert_eq!(
            extract_xml_text(xml, "URLBase"),
            Some("http://192.168.1.123:49152".to_string())
        );
    }

    #[test]
    fn test_ssdp_constants() {
        assert_eq!(SSDP_ADDR, "239.255.255.250");
        assert_eq!(SSDP_PORT, 1900);
        assert_eq!(SEARCH_TARGETS[1], "urn:schemas-upnp-org:device:MediaRenderer:1");
    }
}
