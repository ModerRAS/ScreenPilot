# ScreenPilot

[中文说明](./readme_cn.md)

A small LAN digital signage controller written in **Rust + Tauri** that uses **DLNA / UPnP AV** to control multiple screens.

## Features

- **SSDP Discovery** — automatically finds DLNA MediaRenderer devices on your LAN (auto-refresh every 30 s)
- **DLNA Control** — sends UPnP AVTransport SOAP commands (SetAVTransportURI, Play, Pause, Stop) to each renderer
- **Media HTTP Server** — built-in Axum HTTP server that serves media files from a local `media/` directory
- **Per-Screen Control** — each device is independently controllable from the UI
- **Scenes** — group device→media assignments and apply them with one click

## Architecture

```
ScreenPilot/
├── Cargo.toml                  # Workspace root
├── package.json                # Tauri CLI / frontend deps
├── src/                        # Frontend (HTML + vanilla JS)
│   ├── index.html
│   ├── main.js
│   └── styles.css
├── src-tauri/                  # Rust / Tauri backend
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       ├── lib.rs              # Shared types + helpers exported to main
│       ├── main.rs             # Tauri commands + app entry point
│       ├── discovery.rs        # SSDP discovery
│       ├── dlna.rs             # UPnP AVTransport SOAP commands
│       ├── media_server.rs     # Axum HTTP media file server
│       └── state.rs            # RendererDevice, Scene, AppState
└── media/                      # Drop your .mp4 / .webm files here
    └── README.txt
```

## Prerequisites

### Linux
```bash
sudo apt install libgtk-3-dev libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev
```

### macOS / Windows
Follow the [Tauri prerequisites guide](https://tauri.app/start/prerequisites/).

## Development

```bash
# Install Node deps (Tauri CLI)
npm install

# Run in dev mode (hot-reloads the UI)
npm run dev

# Or build a release binary
npm run build
```

## Usage

1. Drop media files (`.mp4`, `.webm`, etc.) into the `media/` directory next to the binary.
2. Launch ScreenPilot.
3. Click **Discover Devices** to scan the LAN.
4. Select a media file and click **▶ Play** for any discovered renderer.
5. Use **Scenes** to define and apply multi-screen layouts at once.

## Rust Modules

| Module | Responsibility |
|---|---|
| `discovery` | SSDP M-SEARCH, device description XML parsing |
| `dlna` | SOAP envelope building, AVTransport HTTP calls |
| `media_server` | Axum static file server, `local_ip()` helper |
| `state` | `RendererDevice`, `Scene`, `AppState`, `PlaybackStatus` |

## Example SOAP — Play

```xml
POST /upnp/control/AVTransport HTTP/1.1
Content-Type: text/xml; charset="utf-8"
SOAPAction: "urn:schemas-upnp-org:service:AVTransport:1#Play"

<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"
            s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:Play xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <InstanceID>0</InstanceID>
      <Speed>1</Speed>
    </u:Play>
  </s:Body>
</s:Envelope>
```
