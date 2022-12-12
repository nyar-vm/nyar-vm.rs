# rusty-scala

A Scala language frontend for the Nyar VM.

## Overview

`rusty-scala` is a compiler frontend that brings the Scala programming language to the Nyar VM. It provides a modern, expressive runtime for Scala applications, mapping Scala's unique features—like implicit parameters and algebraic data types—to Nyar's native execution engine and algebraic effects.

## Features

- **Modern & Concise**: Full support for Scala's expressive syntax and functional programming features.
- **Strong Typing**: Maintains Scala's powerful type system guarantees during the lowering process to Gaia IR.
- **Managed Object Model**: Maps Scala classes, traits, and objects to Nyar's native object system.
- **JIT Optimization**: Optimized execution of hot Scala code paths via `nyar-jit`.

## Supported Constructs

- **Core Syntax**: `class`, `trait`, `object`, `def`, `val`, `var`.
- **Functional Features**: Lambdas, higher-order functions, and pattern matching.
- **Standard Library**: Initial support for core Scala standard library functions.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run main.scala
```

### Usage as a Library

```rust
use rusty_scala::RustyScalaFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyScalaFrontend::new();
let ast = frontend.parse("def main() { println(\"Hello from Scala on Nyar!\") }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
