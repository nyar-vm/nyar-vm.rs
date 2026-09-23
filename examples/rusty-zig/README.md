# rusty-zig

A Zig language frontend for the Nyar VM.

## Overview

`rusty-zig` is a compiler frontend that brings the Zig programming language—focused on robustness, optimality, and maintainability—to the Nyar VM ecosystem. It allows Zig code to be executed within a managed environment, combining Zig's unique features like `comptime` and explicit error handling with Nyar's advanced JIT and algebraic effects.

## Features

- **Robust & Optimal**: Maps Zig's philosophy of "no hidden control flow" to Nyar's explicit and optimized Gaia IR instructions.
- **Managed Performance**: Benefits from Nyar's multi-tier JIT optimization while maintaining Zig's focus on efficient code generation.
- **Nyar Ecosystem Integration**:
  - **`nyar-jit`**: Optimizes hot Zig functions and loops into native machine instructions.
  - **`nyar-gc`**: Provides optional automatic memory management for managed Zig objects.
  - **Algebraic Effects**: Used to implement Zig's explicit error handling and future concurrency patterns.
- **Modern Tooling**: Leverages Nyar's unified CLI and diagnostic tools for a seamless development experience.

## Supported Constructs

- **Core Syntax**: `fn`, `var`, `const`, `struct`, `enum`, `union`.
- **Control Flow**: `if`, `while`, `for`, `switch`, `defer`.
- **Error Handling**: Mapping Zig's error sets and `try` operator to Nyar's native error system.
- **Comptime**: Basic support for Zig's compile-time code execution, integrated with Nyar's metaprogramming.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run application.zig
```

### Usage as a Library

```rust
use rusty_zig::RustyZigFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyZigFrontend::new();
let result = frontend.parse("fn add(a: i32, b: i32) i32 { return a + b; }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
