# rusty-swift

A Swift language frontend for the Nyar VM.

## Overview

`rusty-swift` is a compiler frontend that brings the Swift programming language—focused on safety, performance, and modern syntax—to the Nyar VM ecosystem. It allows Swift code to be executed within a managed environment, leveraging Nyar's advanced JIT, garbage collection, and algebraic effects for modern application development.

## Features

- **Safe & Fast**: Maps Swift's safety-first philosophy to Nyar's managed runtime and optimized Gaia IR.
- **Modern Object Model**: Robust support for Swift's classes, structs, enums, and protocols.
- **Nyar Ecosystem Integration**:
  - **`nyar-jit`**: Provides multi-tier optimization for hot Swift methods and closures.
  - **`nyar-gc`**: Uses Nyar's precise garbage collector for automatic memory management (mapping Swift's ARC where appropriate).
  - **Algebraic Effects**: Used to implement Swift's `async/await`, error handling (`try/catch`), and future concurrency models.
- **Interoperability**: Designed to interoperate with other Nyar-supported languages like Objective-C (future) and Rust.

## Supported Constructs

- **Core Syntax**: `func`, `var`, `let`, `class`, `struct`, `enum`, `protocol`.
- **Control Flow**: `if`, `guard`, `switch`, `for-in`, `while`.
- **Optionals**: Native support for Swift's optional type system and optional chaining.
- **Generics**: Robust support for Swift's powerful generic programming features.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run application.swift
```

### Usage as a Library

```rust
use rusty_swift::RustySwiftFrontend;
use nyar_types::NyarFrontend;

let frontend = RustySwiftFrontend::new();
let ast = frontend.parse("func square(_ x: Int) -> Int { return x * x }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
