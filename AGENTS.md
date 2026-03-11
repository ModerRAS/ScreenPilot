# PROJECT KNOWLEDGE BASE

**Generated:** 2026-03-10
**Commit:** f822bb5
**Branch:** master

## OVERVIEW

Rust + Axum backend + Vue 3 + ElementUI frontend. LAN digital signage controller using DLNA/UPnP AV to control multiple screens.

## STRUCTURE

```
ScreenPilot/
├── Cargo.toml                  # Workspace root
├── package.json                # pnpm workspace root
├── backend/                   # Rust + Axum API server
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs             # Axum routes + app entry
│       ├── discovery.rs        # SSDP M-SEARCH, device XML parsing
│       ├── dlna.rs             # UPnP AVTransport SOAP commands
│       ├── media_server.rs     # Axum static file server
│       ├── state.rs            # RendererDevice, Scene, AppState
│       └── frontend.rs         # Frontend static serve
├── frontend/                   # Vue 3 + ElementUI + Vite
│   ├── package.json
│   ├── vite.config.ts
│   ├── vitest.config.ts
│   └── src/
│       ├── main.ts             # Vue entry
│       ├── App.vue             # Root component
│       ├── api/                # Axios API client
│       ├── views/              # DevicesView, ScenesView
│       ├── stores/             # Pinia state
│       ├── router/             # Vue Router
│       ├── types/              # TypeScript types
│       └── assets/             # Static assets
└── media/                      # Drop .mp4/.webm files
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Add API route | `backend/src/main.rs` | Add route + handler |
| Modify device state | `backend/src/state.rs` | `RendererDevice`, `Scene`, `AppState` |
| SSDP discovery | `backend/src/discovery.rs` | `discover_renderers()` async fn |
| DLNA SOAP | `backend/src/dlna.rs` | `play_media()`, `pause()`, `stop()` |
| Media server | `backend/src/media_server.rs` | Axum static file on port 8090 |
| Frontend API client | `frontend/src/api/` | Axios instances |
| Vue components | `frontend/src/views/` | DevicesView, ScenesView |
| Pinia stores | `frontend/src/stores/` | State management |

## CODE MAP

| Symbol | Type | Location | Role |
|--------|------|----------|------|
| `AppState` | struct | state.rs:10 | Shared app state |
| `RendererDevice` | struct | state.rs:20 | DLNA device model |
| `Scene` | struct | state.rs:35 | Device→media grouping |
| `discover_renderers` | fn | discovery.rs:50 | SSDP M-SEARCH entry |
| `play_media` | fn | dlna.rs:30 | Send SetAVTransportURI |

## CONVENTIONS

- **No rustfmt.toml** — Uses default Rust formatting
- **No clippy.toml** — Uses default lints
- **Inline tests only** — `#[cfg(test)]` blocks in modules
- **Async runtime** — Tokio, all I/O is async
- **State** — `tokio::sync::Mutex` for shared state
- **Frontend** — Vue 3 Composition API, `<script setup>`

## ANTI-PATTERNS (THIS PROJECT)

- **No forbidden patterns found** — Standard Rust/Vue conventions apply

## UNIQUE STYLES

- pnpm workspace (not npm/yarn)
- Axum serves both API + frontend static (port 8080)
- Media server separate on port 8090
- Vue 3 with Pinia + ElementUI

## COMMANDS

```bash
# Development (two terminals)
cd frontend && pnpm dev      # Vite on 5173
cd backend && cargo run       # Axum on 8080

# Build
cd frontend && pnpm build     # Vue build → backend/src/frontend.rs
cd backend && cargo build --release

# Test
cd frontend && pnpm test      # Vitest
cargo test                    # Rust inline tests
```

## NOTES

- Media server runs on port 8090
- SSDP discovery triggers manually (no auto-refresh)
- API serves frontend static in production