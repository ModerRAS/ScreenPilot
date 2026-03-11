# Backend Knowledge

Rust + Axum API server. DLNA/UPnP control for digital signage.

## Structure

```
backend/
├── Cargo.toml          # Crate: screen-pilot-backend
├── src/
│   ├── main.rs         # Axum routes, 15 endpoints
│   ├── discovery.rs    # SSDP M-SEARCH
│   ├── dlna.rs         # UPnP SOAP commands
│   ├── media_server.rs # Static file server (8090)
│   ├── state.rs        # AppState, RendererDevice, Scene
│   └── frontend.rs     # Serve built Vue app
└── target/             # Build output
```

## Key Files

| Task | File | Notes |
|------|------|-------|
| Add API route | `main.rs` | Add Router::route() + handler |
| SSDP discovery | `discovery.rs` | `discover_renderers()` |
| DLNA control | `dlna.rs` | `play_media()`, `pause()`, `stop()` |
| Media server | `media_server.rs` | Port 8090 |
| State management | `state.rs` | `AppState` with `Mutex<Vec<RendererDevice>>` |

## Routes (main.rs)

- `GET/POST /api/devices`
- `POST /api/devices/discover`
- `POST /api/devices/:uuid/play|pause|stop`
- `GET /api/media`
- `GET/POST/DELETE /api/scenes`
- `POST /api/scenes/:name/apply`
- `GET /api/config/*`
- `GET /web/*` → frontend static

## Testing

Inline `#[cfg(test)]` blocks. Run: `cargo test`

## Notes

- Async: Tokio
- State: `tokio::sync::Mutex<T>`
- No external crates beyond Axum ecosystem
