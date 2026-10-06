# rusty-zig

A Zig language frontend for the Nyar VM.

## Overview

`rusty-zig` brings Zig, focused on robustness, optimality, and maintainability, to the Nyar VM, combining `comptime`, explicit error handling, and Nyar JIT/algebraic effects in a managed environment.

## Features

- **Robust and optimal**: "no hidden control flow" philosophy mapped to explicit Gaia IR.
- **Managed performance**: multi-tier JIT optimization while preserving Zig's code-generation orientation.
- **Nyar ecosystem integration**:
  - **`nyar-jit`**: hot functions and loops compiled to native instructions.
  - **`nyar-gc`**: optional automatic memory management for managed Zig objects.
  - **Algebraic effects**: explicit error handling and future concurrency patterns.
- **Modern toolchain**: unified CLI and diagnostic experience.

## Supported constructs

- **Core syntax**: `fn`, `var`, `const`, `struct`, `enum`, `union`.
- **Control flow**: `if`, `while`, `for`, `switch`, `defer`.
- **Error handling**: error sets and `try` mapped to the Nyar error system.
- **Comptime**: initial support for compile-time execution integrated with Nyar metaprogramming.

## Getting started

### Via Nyar CLI

```bash
nyar run application.zig
```

### As a library

```rust
use rusty_zig::RustyZigFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyZigFrontend::new();
let result = frontend.parse("fn add(a: i32, b: i32) i32 { return a + b; }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
