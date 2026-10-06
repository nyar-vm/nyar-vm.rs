# rusty-nim

A Nim language frontend for the Nyar VM.

## Overview

`rusty-nim` brings Nim, focused on efficiency, expressiveness, and elegant syntax, to the Nyar VM, combining systems-level performance with modern VM safety in a managed JIT environment.

## Features

- **Systems performance + managed safety**: Nim code-generation patterns mapped to optimized Gaia IR.
- **Expressive syntax**: indentation-based syntax and functional features.
- **Nyar ecosystem integration**:
  - **`nyar-jit`**: multi-tier optimization of hot procedures and loops.
  - **`nyar-gc`**: precise GC automatic memory management.
  - **`nyar-aot`**: E-Graph saturation optimization for static Nim modules.
- **Metaprogramming**: Nim compile-time code generation integrated with the Nyar macro system.

## Supported constructs

- **Core syntax**: `proc`, `type`, `var`, `let`, `const`.
- **Control flow**: `if`, `case`, `while`, `for`, `block`.
- **Types**: objects, enums, arrays, sequences.
- **Templates and macros**: initial support for Nim metaprogramming.

## Getting started

### Via Nyar CLI

```bash
nyar run script.nim
```

### As a library

```rust
use rusty_nim::RustyNimFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyNimFrontend::new();
let ast = frontend.parse("proc square(x: int): int = x * x").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
