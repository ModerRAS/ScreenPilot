use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;

use anyhow::{Context, Result};
use log::{debug, warn};
use reqwest::Client;

use crate::state::RendererDevice;

const SSDP_ADDR: &str = "239.255.255.250";
const SSDP_PORT: u16 = 1900;
const SEARCH_TARGET: &str = "urn:schemas-upnp-org:device:MediaRenderer:1";
const MX: u8 = 3;

/// Send an M-SEARCH multicast and collect responses for `timeout` seconds.
/// Returns a list of unique location URLs.
fn ssdp_search(timeout: Duration) -> Result<Vec<String>> {
    let socket = UdpSocket::bind("0.0.0.0:0").context("bind UDP socket")?;
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
        SSDP_ADDR, SSDP_PORT, MX, SEARCH_TARGET
    );

    let dest = SocketAddr::new(
        IpAddr::V4(SSDP_ADDR.parse::<Ipv4Addr>().unwrap()),
        SSDP_PORT,
    );
    socket
        .send_to(request.as_bytes(), dest)
        .context("send M-SEARCH")?;

    let mut locations: Vec<String> = Vec::new();
    let mut buf = [0u8; 4096];

    loop {
        match socket.recv_from(&mut buf) {
            Ok((len, _src)) => {
                let response = String::from_utf8_lossy(&buf[..len]);
                debug!("SSDP response:\n{}", response);
                if let Some(location) = parse_location(&response) {
                    if !locations.contains(&location) {
                        locations.push(location);
                    }
                }
            }
            Err(e) if is_timeout_error(&e) => break,
            Err(e) => {
                warn!("SSDP recv error: {e}");
                break;
            }
        }
    }

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
    let body = client
        .get(location)
        .timeout(Duration::from_secs(5))
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
    })
}

fn extract_xml_text<'a>(xml: &'a str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].to_string())
}

/// Find the AVTransport service controlURL within the device description XML.
fn find_av_transport_url(xml: &str, location: &str) -> Option<String> {
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
    let base = base_url(location);
    if path.starts_with("http") {
        Some(path)
    } else {
        Some(format!("{}{}", base, path))
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
        assert_eq!(extract_xml_text(xml, "friendlyName"), None);
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
    fn test_ssdp_constants() {
        assert_eq!(SSDP_ADDR, "239.255.255.250");
        assert_eq!(SSDP_PORT, 1900);
        assert_eq!(SEARCH_TARGET, "urn:schemas-upnp-org:device:MediaRenderer:1");
    }
}
