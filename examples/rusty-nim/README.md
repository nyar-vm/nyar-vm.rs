# rusty-nim

A Nim language frontend for the Nyar VM.

## Overview

`rusty-nim` is a compiler frontend that brings the Nim programming language—focused on efficiency, expressiveness, and elegant syntax—to the Nyar VM ecosystem. It allows Nim's powerful features to be executed within a managed, JIT-optimized environment, combining the performance of systems programming with the safety of a modern VM.

## Features

- **Systems Performance with Managed Safety**: Maps Nim's efficient code generation patterns to Nyar's optimized Gaia IR.
- **Expressive Syntax**: Robust support for Nim's indentation-based syntax and functional features.
- **Nyar Ecosystem Integration**:
  - **`nyar-jit`**: Provides multi-tier optimization for hot Nim procedures and loops.
  - **`nyar-gc`**: Leverages Nyar's precise garbage collector for automatic memory management.
  - **`nyar-aot`**: Supports E-Graph based saturation optimization for static Nim modules.
- **Metaprogramming**: Integrated with Nyar's native macro system to support Nim's powerful compile-time code generation.

## Supported Constructs

- **Core Syntax**: `proc`, `type`, `var`, `let`, `const`.
- **Control Flow**: `if`, `case`, `while`, `for`, `block`.
- **Types**: Support for objects, enums, arrays, and sequences.
- **Templates & Macros**: Basic support for Nim's metaprogramming features.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run script.nim
```

### Usage as a Library

```rust
use rusty_nim::RustyNimFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyNimFrontend::new();
let ast = frontend.parse("proc square(x: int): int = x * x").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
