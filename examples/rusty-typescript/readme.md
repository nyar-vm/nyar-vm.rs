# rusty-typescript

A TypeScript language frontend for the Nyar VM.

## Overview

`rusty-typescript` targets modern TypeScript with decorators and JSX, supporting both JIT execution on `nyar-vm` and AOT compilation to WebAssembly.

## Features

- **Modern TypeScript**: decorators, JSX/TSX, and contemporary ES features.
- **Dual execution**:
  - **JIT**: lowers to unified IR and runs on `nyar-vm` with dynamic optimization.
  - **AOT**: emits WebAssembly for browser or edge deployment.
- **Type system**: integrates with `nyar-aot` for constraint analysis.
- **WASM interop**: WASM modules via `wit-bindgen` integration paths.
- **Optimization**: E-Graph saturation improves emitted code quality.

## Supported constructs

- **TS features**: interfaces, enums, type aliases, generics, decorators.
- **Modern JavaScript**: classes, ESM, async/await, destructuring.
- **JSX/TSX**: React-style components and templates.
- **Metaprogramming**: hooks into Nyar macro and reflection facilities where enabled.

## Getting started

### Via Nyar CLI

```bash
nyar run script.ts
```

### Compile to WebAssembly

```bash
nyar compile script.ts -o output.wasm
```

### As a library

```rust
use rusty_typescript::RustyTypescriptFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyTypescriptFrontend::new();
let ast = frontend.parse("const x: number = 42; console.log(x);").unwrap();
```

## License

Licensed under MIT OR Apache-2.0.
