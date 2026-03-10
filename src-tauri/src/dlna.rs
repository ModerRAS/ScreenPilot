use anyhow::{Context, Result};
use log::debug;
use reqwest::Client;
use std::time::Duration;

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
async fn send_soap(
    client: &Client,
    url: &str,
    action: &str,
    body: &str,
) -> Result<String> {
    let soap_action = format!(
        "\"urn:schemas-upnp-org:service:AVTransport:1#{}\"",
        action
    );

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

/// Full play sequence: Stop → SetAVTransportURI → Play.
pub async fn play_media(
    client: &Client,
    av_transport_url: &str,
    media_uri: &str,
) -> Result<()> {
    // Stop first (best-effort – ignore errors)
    let _ = stop(client, av_transport_url).await;
    set_av_transport_uri(client, av_transport_url, media_uri).await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_escape() {
        assert_eq!(xml_escape("a&b<c>d\"e'f"), "a&amp;b&lt;c&gt;d&quot;e&apos;f");
        assert_eq!(xml_escape("http://host/media/ad.mp4"), "http://host/media/ad.mp4");
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
    fn test_soap_envelope_pause() {
        let body = soap_envelope("Pause", "");
        assert!(body.contains("<u:Pause xmlns:u="));
    }
}
