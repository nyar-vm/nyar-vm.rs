# rusty-kotlin

A Kotlin language frontend for the Nyar VM.

## Overview

`rusty-kotlin` brings Kotlin to the Nyar VM, mapping null safety, coroutines, and other features to the Nyar execution engine and algebraic effects.

## Features

- **Modern and concise**: Kotlin expressiveness and functional features.
- **Null safety**: null-safety guarantees preserved when lowering to Gaia IR.
- **Coroutines**: coroutines and `suspend` mapped to Nyar algebraic effects and continuations.
- **Managed object model**: classes, interfaces, and data classes mapped to the Nyar object system.
- **JIT optimization**: hot Kotlin paths optimized via `nyar-jit`.

## Supported constructs

- **Core syntax**: `class`, `data class`, `interface`, `fun`, `val`, `var`.
- **Null safety**: nullable types `?`, safe call `?.`, Elvis `?:`.
- **Functional**: lambdas, higher-order functions, extension functions.
- **Coroutines**: `suspend` and structured concurrency.
- **Standard library**: initial support for the Kotlin standard library core.

## Getting started

### Via Nyar CLI

```bash
nyar run main.kt
```

### As a library

```rust
use rusty_kotlin::RustyKotlinFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyKotlinFrontend::new();
let ast = frontend.parse("fun main() { println(\"Hello from Kotlin on Nyar!\") }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
