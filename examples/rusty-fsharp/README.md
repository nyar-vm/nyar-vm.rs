# rusty-fsharp

An F# language frontend for the Nyar VM.

## Overview

`rusty-fsharp` is a compiler frontend that brings the elegance and power of F#—a functional-first language—to the Nyar VM. It enables developers to write concise, correct, and performant code, leveraging Nyar's advanced runtime features like algebraic effects and multi-tier JIT optimization.

## Features

- **Functional-First Design**: Robust support for F#'s core functional programming constructs, including immutability, first-class functions, and pattern matching.
- **Algebraic Effects Integration**: Maps F#'s computation expressions (like `async { ... }` or `seq { ... }`) to Nyar's native algebraic effects and delimited continuations.
- **Nyar Ecosystem Integration**:
  - **`nyar-jit`**: Provides high-performance execution for hot functional code paths.
  - **`nyar-gc`**: Automatic, precise memory management for complex data structures and closures.
  - **`nyar-aot`**: Supports E-Graph based optimization for pure functional transformations.
- **Type Safety**: Leverages F#'s strong type system, mapped to Nyar's internal type representation.

## Supported Constructs

- **Core Syntax**: `let` bindings, `module`, `type` definitions (records, unions).
- **Functions**: Anonymous functions, currying, and partial application.
- **Pattern Matching**: Exhaustive pattern matching on records and discriminated unions.
- **Computation Expressions**: Mapping to Nyar's native effect handler system.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run script.fs
```

### Usage as a Library

```rust
use rusty_fsharp::RustyFSharpFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyFSharpFrontend::new();
let ast = frontend.parse("let square x = x * x").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
