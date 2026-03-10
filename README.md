# ScreenPilot

[中文说明](./readme_cn.md)

A LAN digital signage controller built with **Rust + Axum** (backend) and **Vue 3 + ElementUI** (frontend), using **DLNA / UPnP AV** to control multiple screens.

## Features

- **SSDP Discovery** — automatically finds DLNA MediaRenderer devices on your LAN (manual trigger)
- **DLNA Control** — sends UPnP AVTransport SOAP commands (SetAVTransportURI, Play, Pause, Stop) to each renderer
- **Media HTTP Server** — built-in Axum HTTP server that serves media files from a local `media/` directory (port 8090)
- **Per-Screen Control** — each device is independently controllable from the UI
- **Scenes** — group device→media assignments and apply them with one click

## Architecture

```
ScreenPilot/
├── Cargo.toml                  # Workspace root
├── package.json                # Root package (for pnpm workspace)
├── backend/                   # Rust + Axum API server
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs             # Axum routes + app entry
│       ├── discovery.rs       # SSDP discovery
│       ├── dlna.rs            # UPnP AVTransport SOAP commands
│       ├── media_server.rs    # Axum media file server
│       └── state.rs           # RendererDevice, Scene, AppState
├── frontend/                  # Vue 3 + ElementUI + Vite + pnpm
│   ├── package.json
│   ├── vite.config.ts
│   └── src/
│       ├── main.ts            # Vue entry point
│       ├── App.vue            # Root component
│       ├── api/               # API client (axios)
│       ├── views/             # DevicesView, ScenesView
│       ├── stores/            # Pinia state
│       └── types/             # TypeScript types
└── media/                    # Drop your .mp4 / .webm files here
```

## Prerequisites

- **Rust** — Install via [rustup](https://rustup.rs/)
- **Node.js** — v20+
- **pnpm** — Install via `npm install -g pnpm`

## Development

```bash
# Terminal 1: Build frontend first
cd frontend && pnpm build

# Terminal 2: Start backend (serves both API and frontend on port 8080)
cd backend && cargo run
```

Then open http://localhost:8080/web in your browser.

## Build

```bash
# Build frontend
cd frontend && pnpm build

# Build backend
cd backend && cargo build --release

# Or use pnpm workspace to build both
pnpm --filter frontend build
pnpm --filter backend build
```

## Production

```bash
# Run backend
./backend/target/release/screen-pilot-backend

# Serve frontend dist (e.g., with nginx, or use Vite preview)
cd frontend && pnpm preview
```

## Usage

1. Drop media files (`.mp4`, `.webm`, etc.) into the `media/` directory.
2. Start the backend: `cargo run --manifest-path backend/Cargo.toml`
3. Start the frontend: `pnpm --dir frontend dev`
4. Open http://localhost:5173
5. Click **Discover Devices** to scan the LAN.
6. Select a media file and click **▶ Play** for any discovered renderer.
7. Use **Scenes** to define and apply multi-screen layouts at once.

## API Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/devices` | List all discovered devices |
| POST | `/api/devices/discover` | Trigger SSDP discovery |
| POST | `/api/devices/:uuid/play` | Play media on device |
| POST | `/api/devices/:uuid/pause` | Pause playback |
| POST | `/api/devices/:uuid/stop` | Stop playback |
| GET | `/api/media` | List media files |
| GET | `/api/scenes` | List saved scenes |
| POST | `/api/scenes` | Save a scene |
| DELETE | `/api/scenes/:name` | Delete a scene |
| POST | `/api/scenes/:name/apply` | Apply scene to devices |
| GET | `/api/config/media-server-url` | Get media server URL |

## Rust Modules (Backend)

| Module | Responsibility |
|--------|----------------|
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
