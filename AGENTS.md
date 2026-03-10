# PROJECT KNOWLEDGE BASE

**Generated:** 2026-03-10
**Commit:** 9021994
**Branch:** master

## OVERVIEW

Tauri 2.0 desktop app — LAN digital signage controller using DLNA/UPnP AV to control multiple screens. Rust backend + vanilla JS frontend.

## STRUCTURE

```
ScreenPilot/
├── Cargo.toml                  # Workspace root (member: src-tauri)
├── package.json                # Tauri CLI + frontend deps
├── src/                        # Frontend (HTML + vanilla JS)
│   ├── index.html
│   ├── main.js
│   └── styles.css
├── src-tauri/
│   ├── Cargo.toml              # Crate: screen-pilot
│   ├── tauri.conf.json
│   └── src/
│       ├── lib.rs              # Exports modules + shared types
│       ├── main.rs             # Binary entry + 11 Tauri commands
│       ├── discovery.rs        # SSDP M-SEARCH, device XML parsing
│       ├── dlna.rs            # UPnP AVTransport SOAP commands
│       ├── media_server.rs    # Axum HTTP media file server
│       └── state.rs           # RendererDevice, Scene, AppState
└── media/                      # Drop .mp4/.webm files here
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Add new Tauri command | `src-tauri/src/main.rs` | Add fn + register in `invoke_handler!` |
| Modify device state | `src-tauri/src/state.rs` | `RendererDevice`, `Scene`, `AppState` types |
| SSDP discovery logic | `src-tauri/src/discovery.rs` | `discover_renderers()` async fn |
| DLNA SOAP commands | `src-tauri/src/dlna.rs` | `play_media()`, `pause()`, `stop()` |
| Media HTTP server | `src-tauri/src/media_server.rs` | Axum on port 8090 |
| Frontend UI | `src/main.js` | Vanilla JS, DOM manipulation |

## CODE MAP

| Symbol | Type | Location | Role |
|--------|------|----------|------|
| `TauriAppState` | struct | lib.rs:14 | Managed state for Tauri |
| `SceneApplyResult` | struct | lib.rs:20 | Scene apply response |
| `RendererDevice` | struct | state.rs | DLNA device model |
| `discover_renderers` | fn | discovery.rs | SSDP discovery entry |
| `play_media` | fn | dlna.rs | Send SetAVTransportURI |

## CONVENTIONS

- **No rustfmt.toml** — Uses default Rust formatting
- **No clippy.toml** — Uses default lints
- **Inline tests only** — `#[cfg(test)]` blocks in modules, no `tests/` dir
- **lib/main split** — `lib.rs` exports modules, `main.rs` contains commands
- **Async runtime** — Tokio, all I/O is async
- **State** — `tokio::sync::Mutex` for shared state

## ANTI-PATTERNS (THIS PROJECT)

- **Don't remove line 2 in main.rs** — `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` required to suppress console on Windows release

## UNIQUE STYLES

- Single workspace member (`src-tauri` only)
- Vanilla JS frontend (no React/Vue/Svelte)
- Media directory resolution: `resolve_media_dir()` checks exe dir first, then dev folder

## COMMANDS

```bash
npm run dev    # Hot-reload development
npm run build  # Production release
cargo test     # Run inline tests
```

## NOTES

- Media server runs on port 8090 (embedded Axum)
- SSDP discovery refreshes every 30 seconds (background loop in main.rs)
- 11 Tauri commands: discover_devices, get_devices, play_on_device, pause_device, stop_device, list_media, get_scenes, save_scene, delete_scene, apply_scene, get_media_server_url
- 不管用什么命令，都不许以`>nul`结尾