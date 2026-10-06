# nyar-napi

Node-API (**N-API**) native binding layer for the Nyar runtime.

## Overview

`nyar-napi` produces the native `.node` sidecar loaded by the main `nyar` npm package on matching platforms. It re-exports `nyar-runner`. There is **no** CLI binary here—commands are assembled in `projects/packages/nyar` from per-platform collect artifacts.

## Build

```bash
cargo build -p nyar-napi --release
pnpm build:napi   # from projects/packages
```

## API surface

Placeholder export (`napi_placeholder`) until `#[napi]` wrappers (run / compile / inspect) land. Business logic stays in Rust host modules shared with other targets.

## Boundaries

- Platform sidecar—not imported directly from app code; use the umbrella `nyar` package.
- Compilation pipelines remain in `nyar-language` / `nyar-emitter`.
