# rusty-rust (Mini Rust)

A Rust language frontend for the Nyar VM.

## Overview

`rusty-rust` (Mini Rust) executes a subset of Rust on the Nyar VM. The parse and lowering pipeline transforms Rust source into unified IR, benefiting from Nyar runtime optimizations (JIT, AOT, GC).

## Features

- **Safe subset**: ownership concepts, pattern matching, and traits.
- **Advanced AOT**: integrates with `nyar-aot` and E-Graph optimization for bytecode modules.
- **Type-safe lowering**: uses `chomsky-uir` for constraint analysis during lowering.
- **Algebraic effects**: maps Rust async/future models to Nyar native effects.
- **Zero-cost abstractions**: aims to preserve Rust performance characteristics on the VM.

## Supported constructs

- **Core syntax**: `let`, `fn`, `struct`, `enum`, `impl`.
- **Control flow**: `if`, `loop`, `while`, `for`, `match`.
- **Ownership**: references and compile-time borrow checking during lowering.
- **Traits**: trait polymorphism via witness tables.
- **Macros**: basic declarative macro support.

## Getting started

### Via Nyar CLI

```bash
nyar run main.rs
```

### As a library

```rust
use rusty_rust::MiniRustFrontend;
use nyar_types::NyarFrontend;

let frontend = MiniRustFrontend::new();
let ast = frontend.parse("fn main() { println!(\"Hello from Rust!\"); }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
