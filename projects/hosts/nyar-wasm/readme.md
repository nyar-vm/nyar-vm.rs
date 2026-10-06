# nyar-wasm

WebAssembly **cdylib/rlib** binding layer for the Nyar runtime.

## Overview

`nyar-wasm` builds the Wasm GC-facing artifact consumed by `@game-gpt/nyar` platform packages (`nyar-wasm32-wasi`). It re-exports `nyar-runner` for shared host dispatch. There is **no** `[[bin]]` or `main` in this crate—the user-facing CLI lives under `projects/packages/`.

## Build

```bash
cargo build -p nyar-wasm --release
pnpm build:wasm   # from projects/packages — collects into platform npm packages
```

## API surface

Currently exposes a placeholder (`wasm_placeholder`) while Wasm export wiring is expanded. Actual lowering and module bytes are produced by the Nyar compile pipeline, not hand-written in this crate.

## Boundaries

- Not a public import target for application code—install the main `nyar` npm package instead.
- Does not implement the Valkyrie compiler (see `nyar-language`).
