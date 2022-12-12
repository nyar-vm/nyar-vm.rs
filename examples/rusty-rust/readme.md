# rusty-rust (Mini Rust)

A Rust language frontend for the Nyar VM.

## Overview

`rusty-rust` (also known as Mini Rust) is a compiler frontend that allows a subset of the Rust programming language to be executed on the Nyar VM. It provides a robust parsing and lowering pipeline that transforms Rust source code into Gaia IR, benefiting from Nyar's advanced runtime optimizations.

## Features

- **Safe subset of Rust**: Supports core Rust features including ownership concepts, pattern matching, and traits.
- **Advanced AOT Compilation**: Utilizes `nyar-aot` and E-Graph optimization to generate highly optimized bytecode modules.
- **Type-Safe Lowering**: Leverages `chomsky-uir` for constraint analysis during the lowering process.
- **Algebraic Effects Integration**: Maps Rust's future/async system to Nyar's native algebraic effects.
- **Zero-Cost Abstractions**: Aims to maintain Rust's promise of high performance even when running on a virtual machine.

## Supported Constructs

- **Core Syntax**: `let` bindings, `fn` definitions, `struct`, `enum`, `impl`.
- **Control Flow**: `if`, `loop`, `while`, `for`, `match`.
- **Ownership**: Support for references and basic borrow checking logic during compilation.
- **Traits**: Implementation of trait-based polymorphism via Nyar's witness tables.
- **Macros**: Support for basic declarative macros.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run main.rs
```

### Usage as a Library

```rust
use rusty_rust::MiniRustFrontend;
use nyar_types::NyarFrontend;

let frontend = MiniRustFrontend::new();
let ast = frontend.parse("fn main() { println!(\"Hello from Rust!\"); }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
