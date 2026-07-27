use anyhow::{Context, Result};
use log::debug;
use reqwest::Client;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportInfo {
    pub current_transport_state: String,
    pub current_transport_status: String,
    pub current_speed: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaInfo {
    pub current_uri: String,
}

/// Build the SOAP envelope for a UPnP AVTransport action.
fn soap_envelope(action: &str, args: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"
            s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:{action} xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <InstanceID>0</InstanceID>
      {args}
    </u:{action}>
  </s:Body>
</s:Envelope>"#,
        action = action,
        args = args
    )
}

/// Send a SOAP action to the given AVTransport control URL.
async fn send_soap(client: &Client, url: &str, action: &str, body: &str) -> Result<String> {
    let soap_action = format!("\"urn:schemas-upnp-org:service:AVTransport:1#{}\"", action);

    debug!("SOAP → {} : {} ", url, action);
    debug!("SOAP body:\n{}", body);

    let response = client
        .post(url)
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .header("SOAPAction", &soap_action)
        .timeout(Duration::from_secs(8))
        .body(body.to_string())
        .send()
        .await
        .with_context(|| format!("POST SOAP to {url}"))?;

    let status = response.status();
    let text = response.text().await.context("read SOAP response")?;
    debug!("SOAP ← {} : {}", status, text);

    if !status.is_success() {
        anyhow::bail!("SOAP error {status}: {text}");
    }
    Ok(text)
}

/// SetAVTransportURI — tell the renderer what media URI to prepare.
pub async fn set_av_transport_uri(
    client: &Client,
    av_transport_url: &str,
    media_uri: &str,
) -> Result<()> {
    let args = format!(
        "<CurrentURI>{}</CurrentURI>\
         <CurrentURIMetaData></CurrentURIMetaData>",
        xml_escape(media_uri)
    );
    let body = soap_envelope("SetAVTransportURI", &args);
    send_soap(client, av_transport_url, "SetAVTransportURI", &body).await?;
    Ok(())
}

/// Play — start playback at normal speed.
pub async fn play(client: &Client, av_transport_url: &str) -> Result<()> {
    let args = "<Speed>1</Speed>";
    let body = soap_envelope("Play", args);
    send_soap(client, av_transport_url, "Play", &body).await?;
    Ok(())
}

/// Pause — pause playback.
pub async fn pause(client: &Client, av_transport_url: &str) -> Result<()> {
    let body = soap_envelope("Pause", "");
    send_soap(client, av_transport_url, "Pause", &body).await?;
    Ok(())
}

/// Stop — stop playback.
pub async fn stop(client: &Client, av_transport_url: &str) -> Result<()> {
    let body = soap_envelope("Stop", "");
    send_soap(client, av_transport_url, "Stop", &body).await?;
    Ok(())
}

/// GetTransportInfo — read the renderer's actual AVTransport state.
pub async fn get_transport_info(client: &Client, av_transport_url: &str) -> Result<TransportInfo> {
    let body = soap_envelope("GetTransportInfo", "");
    let response = send_soap(client, av_transport_url, "GetTransportInfo", &body).await?;

    Ok(TransportInfo {
        current_transport_state: extract_xml_text(&response, "CurrentTransportState")
            .unwrap_or_else(|| "UNKNOWN".to_string()),
        current_transport_status: extract_xml_text(&response, "CurrentTransportStatus")
            .unwrap_or_else(|| "UNKNOWN".to_string()),
        current_speed: extract_xml_text(&response, "CurrentSpeed")
            .unwrap_or_else(|| "1".to_string()),
    })
}

/// GetMediaInfo — read the renderer's current media URI.
pub async fn get_media_info(client: &Client, av_transport_url: &str) -> Result<MediaInfo> {
    let body = soap_envelope("GetMediaInfo", "");
    let response = send_soap(client, av_transport_url, "GetMediaInfo", &body).await?;

    Ok(MediaInfo {
        current_uri: extract_xml_text(&response, "CurrentURI").unwrap_or_default(),
    })
}

/// SetPlayMode — request a renderer-side play mode when supported.
pub async fn set_play_mode(client: &Client, av_transport_url: &str, play_mode: &str) -> Result<()> {
    let args = format!("<NewPlayMode>{}</NewPlayMode>", xml_escape(play_mode));
    let body = soap_envelope("SetPlayMode", &args);
    send_soap(client, av_transport_url, "SetPlayMode", &body).await?;
    Ok(())
}

async fn set_play_mode_best_effort(
    client: &Client,
    av_transport_url: &str,
    play_mode: &str,
) -> bool {
    match tokio::time::timeout(
        Duration::from_secs(1),
        set_play_mode(client, av_transport_url, play_mode),
    )
    .await
    {
        Ok(Ok(_)) => true,
        Ok(Err(e)) => {
            debug!(
                "Renderer did not accept AVTransport SetPlayMode {}: {}",
                play_mode, e
            );
            false
        }
        Err(_) => {
            debug!(
                "Renderer timed out on AVTransport SetPlayMode {}, continuing playback",
                play_mode
            );
            false
        }
    }
}

/// Resume a renderer that unexpectedly stopped the current loop URI.
pub async fn replay_media(client: &Client, av_transport_url: &str, media_uri: &str) -> Result<()> {
    set_av_transport_uri(client, av_transport_url, media_uri).await?;
    play(client, av_transport_url).await
}

/// Full play sequence: Stop → SetAVTransportURI → optional SetPlayMode → Play.
pub async fn play_media(
    client: &Client,
    av_transport_url: &str,
    media_uri: &str,
    loop_playback: bool,
    loop_media_uri: Option<&str>,
) -> Result<()> {
    // Stop first (best-effort – ignore errors)
    let _ = stop(client, av_transport_url).await;

    let playback_uri = if loop_playback {
        loop_media_uri.unwrap_or(media_uri)
    } else {
        media_uri
    };
    set_av_transport_uri(client, av_transport_url, playback_uri).await?;

    if !loop_playback {
        let _ = set_play_mode_best_effort(client, av_transport_url, "NORMAL").await;
    }

    play(client, av_transport_url).await?;
    Ok(())
}

/// Minimal XML character escaping for attribute/element text content.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn extract_xml_text(xml: &str, tag: &str) -> Option<String> {
    let start = format!("<{}>", tag);
    let end = format!("</{}>", tag);
    let start_index = xml.find(&start)? + start.len();
    let end_index = xml[start_index..].find(&end)? + start_index;
    Some(xml[start_index..end_index].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_escape() {
        assert_eq!(
            xml_escape("a&b<c>d\"e'f"),
            "a&amp;b&lt;c&gt;d&quot;e&apos;f"
        );
        assert_eq!(
            xml_escape("http://host/media/ad.mp4"),
            "http://host/media/ad.mp4"
        );
    }

    #[test]
    fn test_soap_envelope_play() {
        let body = soap_envelope("Play", "<Speed>1</Speed>");
        assert!(body.contains("<u:Play xmlns:u="));
        assert!(body.contains("<Speed>1</Speed>"));
        assert!(body.contains("<InstanceID>0</InstanceID>"));
    }

    #[test]
    fn test_soap_envelope_set_uri() {
        let args = "<CurrentURI>http://192.168.1.1:8090/media/ad.mp4</CurrentURI><CurrentURIMetaData></CurrentURIMetaData>";
        let body = soap_envelope("SetAVTransportURI", args);
        assert!(body.contains("<u:SetAVTransportURI xmlns:u="));
        assert!(body.contains("ad.mp4"));
    }

    #[test]
    fn test_soap_envelope_stop() {
        let body = soap_envelope("Stop", "");
        assert!(body.contains("<u:Stop xmlns:u="));
    }

    #[test]
    fn test_soap_envelope_set_play_mode() {
        let body = soap_envelope("SetPlayMode", "<NewPlayMode>REPEAT_ONE</NewPlayMode>");
        assert!(body.contains("<u:SetPlayMode xmlns:u="));
        assert!(body.contains("<NewPlayMode>REPEAT_ONE</NewPlayMode>"));
    }

    #[test]
    fn test_soap_envelope_get_transport_info() {
        let body = soap_envelope("GetTransportInfo", "");
        assert!(body.contains("<u:GetTransportInfo xmlns:u="));
        assert!(body.contains("<InstanceID>0</InstanceID>"));
    }

    #[test]
    fn test_soap_envelope_get_media_info() {
        let body = soap_envelope("GetMediaInfo", "");
        assert!(body.contains("<u:GetMediaInfo xmlns:u="));
        assert!(body.contains("<InstanceID>0</InstanceID>"));
    }

    #[test]
    fn test_extract_xml_text() {
        let xml = "<CurrentTransportState>PLAYING</CurrentTransportState>";
        assert_eq!(
            extract_xml_text(xml, "CurrentTransportState"),
            Some("PLAYING".to_string())
        );
    }

    #[test]
    fn test_soap_envelope_pause() {
        let body = soap_envelope("Pause", "");
        assert!(body.contains("<u:Pause xmlns:u="));
    }

    #[test]
    fn test_soap_envelope_empty_args() {
        let body = soap_envelope("Stop", "");
        assert!(body.contains("</u:Stop>"));
        assert!(body.contains("<InstanceID>0</InstanceID>"));
    }

    #[test]
    fn test_xml_escape_all_characters() {
        assert_eq!(xml_escape("&<>\"'"), "&amp;&lt;&gt;&quot;&apos;");
    }

    #[test]
    fn test_xml_escape_no_escape_needed() {
        assert_eq!(xml_escape("hello world"), "hello world");
    }

    #[test]
    fn test_xml_escape_already_escaped() {
        assert_eq!(xml_escape("&amp;"), "&amp;amp;");
    }

    #[test]
    fn test_soap_envelope_with_complex_args() {
        let args = "<CurrentURI>http://example.com/video.mp4</CurrentURI><CurrentURIMetaData>&lt;&gt;</CurrentURIMetaData>";
        let body = soap_envelope("SetAVTransportURI", args);
        assert!(body.contains("SetAVTransportURI"));
        assert!(body.contains("CurrentURI"));
        assert!(body.contains("video.mp4"));
    }

    #[test]
    fn test_soap_action_header_format() {
        let action = "Play";
        let soap_action = format!("\"urn:schemas-upnp-org:service:AVTransport:1#{}\"", action);
        assert_eq!(
            soap_action,
            "\"urn:schemas-upnp-org:service:AVTransport:1#Play\""
        );
    }

    #[test]
    fn test_soap_envelope_se_complete() {
        let body = soap_envelope("Seek", "<Unit>REL_TIME</Unit><Target>00:01:30</Target>");
        assert!(body.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\"?>"));
        assert!(body.contains("<s:Envelope"));
        assert!(body.contains("</s:Envelope>"));
        assert!(body.contains("<u:Seek xmlns:u="));
    }
}
