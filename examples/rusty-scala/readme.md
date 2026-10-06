# rusty-scala

A Scala language frontend for the Nyar VM.

## Overview

`rusty-scala` brings Scala to the Nyar VM, mapping implicit parameters, algebraic data types, and other Scala features to the Nyar execution engine and algebraic effects.

## Features

- **Modern and concise**: Scala expressiveness and functional features.
- **Strong typing**: Scala type guarantees preserved when lowering to Gaia IR.
- **Managed object model**: classes, traits, and objects mapped to the Nyar object system.
- **JIT optimization**: hot Scala paths optimized via `nyar-jit`.

## Supported constructs

- **Core syntax**: `class`, `trait`, `object`, `def`, `val`, `var`.
- **Functional**: lambdas, higher-order functions, pattern matching.
- **Standard library**: initial support for the Scala standard library core.

## Getting started

### Via Nyar CLI

```bash
nyar run main.scala
```

### As a library

```rust
use rusty_scala::RustyScalaFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyScalaFrontend::new();
let ast = frontend.parse("def main() { println(\"Hello from Scala on Nyar!\") }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
