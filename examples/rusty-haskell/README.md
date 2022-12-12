# rusty-haskell

An Haskell language frontend for the Nyar VM.

## Overview

`rusty-haskell` is a compiler frontend that brings the elegance and power of Haskell—a purely functional language—to the Nyar VM. It enables developers to write concise, correct, and performant code, leveraging Nyar's advanced runtime features like algebraic effects and multi-tier JIT optimization.

## Features

- **Purely Functional**: Robust support for Haskell's core functional programming constructs, including laziness, first-class functions, and pattern matching.
- **Algebraic Effects Integration**: Maps Haskell's monads and effect systems to Nyar's native algebraic effects and delimited continuations.
- **Nyar Ecosystem Integration**:
  - **`nyar-jit`**: Provides high-performance execution for hot functional code paths.
  - **`nyar-gc`**: Automatic, precise memory management for complex data structures and closures.
  - **`nyar-aot`**: Supports E-Graph based optimization for pure functional transformations.
- **Strong Typing**: Leverages Haskell's advanced type system (HKTs, Type Classes), mapped to Nyar's internal type representation.

## Supported Constructs

- **Core Syntax**: `let` bindings, `module`, `data` and `type` declarations.
- **Functions**: Anonymous functions, currying, and partial application.
- **Pattern Matching**: Exhaustive pattern matching on data types.
- **Type Classes**: (WIP) Mapping to Nyar's trait system.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run script.hs
```

### Usage as a Library

```rust
use rusty_haskell::RustyHaskellFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyHaskellFrontend::new();
let ast = frontend.parse("square x = x * x").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
