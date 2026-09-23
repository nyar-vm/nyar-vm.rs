# rusty-typescript

A TypeScript language frontend for the Nyar VM.

## Overview

`rusty-typescript` is a sophisticated compiler frontend for the TypeScript language, targeting the Nyar VM. It supports modern TypeScript features, including decorators and JSX, and provides both JIT (via `nyar-vm`) and AOT (compiling to WebAssembly) execution paths.

## Features

- **Modern TypeScript Support**: Includes support for decorators, JSX/TSX, and modern ES features.
- **Dual-Mode Execution**:
  - **JIT Mode**: Compiles to Gaia IR for execution on `nyar-vm` with dynamic optimizations.
  - **AOT Mode**: Compiles directly to WebAssembly (WASM) for deployment in browser or edge environments.
- **Robust Type System**: Integrated with `nyar-aot` for constraint-based type analysis and optimization.
- **WASM Interop**: Built-in support for generating WASM modules using `wit-bindgen`.
- **Optimization Pipeline**: Uses E-Graph equality saturation for high-quality code generation.

## Supported Constructs

- **TS-Specific Features**: Interfaces, Enums, Type Aliases, Generics, Decorators.
- **Modern JavaScript**: Classes, Modules (ESM), Async/Await, Destructuring.
- **JSX/TSX**: Direct support for React-style components and templating.
- **Metaprogramming**: Integrated with Nyar's macro and reflection systems.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run script.ts
```

### Compiling to WebAssembly

```bash
nyar compile script.ts -o output.wasm
```

### Usage as a Library

```rust
use rusty_typescript::RustyTypescriptFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyTypescriptFrontend::new();
let ast = frontend.parse("const x: number = 42; console.log(x);").unwrap();
// Lower to tree and generate code
```

## License

Licensed under MIT OR Apache-2.0.
