# rusty-kotlin

A Kotlin language frontend for the Nyar VM.

## Overview

`rusty-kotlin` is a compiler frontend that brings the Kotlin programming language to the Nyar VM. It provides a modern, expressive runtime for Kotlin applications, mapping Kotlin's unique features—like null safety and coroutines—to Nyar's native execution engine and algebraic effects.

## Features

- **Modern & Concise**: Full support for Kotlin's expressive syntax and functional programming features.
- **Null Safety**: Maintains Kotlin's null safety guarantees during the lowering process to Gaia IR.
- **Coroutine Support**: Maps Kotlin's coroutines and suspending functions to Nyar's native algebraic effects and continuations.
- **Managed Object Model**: Maps Kotlin classes, interfaces, and data classes to Nyar's native object system.
- **JIT Optimization**: Optimized execution of hot Kotlin code paths via `nyar-jit`.

## Supported Constructs

- **Core Syntax**: `class`, `data class`, `interface`, `fun`, `val`, `var`.
- **Null Safety**: Support for nullable types (`?`), safe calls (`?.`), and elvis operator (`?:`).
- **Functional Features**: Lambdas, higher-order functions, and extension functions.
- **Coroutines**: Native support for `suspend` functions and structured concurrency.
- **Standard Library**: Initial support for core Kotlin standard library functions.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run main.kt
```

### Usage as a Library

```rust
use rusty_kotlin::RustyKotlinFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyKotlinFrontend::new();
let ast = frontend.parse("fun main() { println(\"Hello from Kotlin on Nyar!\") }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
