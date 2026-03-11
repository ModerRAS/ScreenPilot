# Frontend Knowledge

Vue 3 + ElementUI + Vite. SPA for DLNA device control.

## Structure

```
frontend/
├── package.json
├── vite.config.ts
├── vitest.config.ts
└── src/
    ├── main.ts        # Vue entry
    ├── App.vue        # Root component
    ├── api/           # Axios client
    ├── views/         # DevicesView, ScenesView
    ├── stores/        # Pinia (app.ts)
    ├── router/        # Vue Router
    ├── types/         # TypeScript interfaces
    └── assets/        # CSS, images
```

## Key Files

| Task | File | Notes |
|------|------|-------|
| API calls | `src/api/index.ts` | Axios instance |
| Device UI | `src/views/DevicesView.vue` | Device list, playback controls |
| Scene UI | `src/views/ScenesView.vue` | Scene management |
| State | `src/stores/app.ts` | Pinia store |
| Types | `src/types/index.ts` | `RendererDevice`, `Scene`, `MediaFile` |

## Commands

```bash
pnpm dev      # Vite dev server (5173)
pnpm build   # Build → dist/
pnpm test    # Vitest
```

## Testing

Vitest configured. Test files: `src/**/__tests__/*`

## Notes

- Vue 3 Composition API (`<script setup>`)
- ElementUI components
- Pinia for state
- TypeScript
