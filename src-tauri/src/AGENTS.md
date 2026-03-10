# Rust Source Modules

**Parent:** `./AGENTS.md`

## OVERVIEW

Core Rust modules for DLNA/UPnP AV control. All async (Tokio).

## MODULES

| Module | File | Responsibility |
|--------|------|----------------|
| `discovery` | discovery.rs | SSDP M-SEARCH, device XML parsing |
| `dlna` | dlna.rs | UPnP AVTransport SOAP (play, pause, stop) |
| `media_server` | media_server.rs | Axum HTTP server (port 8090) |
| `state` | state.rs | `RendererDevice`, `Scene`, `AppState`, `PlaybackStatus` |

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Add Tauri command | main.rs | Add fn + register in `invoke_handler!` |
| State types | state.rs | Device/scene data structures |
| HTTP client | lib.rs:16 | `Mutex<Client>` in TauriAppState |

## CONVENTIONS

- Inline tests: `#[cfg(test)]` blocks only
- State access: `shared.read().await` / `shared.write().await`
- HTTP requests: `reqwest::Client` from TauriAppState

## ANTI-PATTERNS

- Don't block async with `.wait()` — use `.await`
- Don't clone inside loops — pre-clone outside
