# rusty-haskell

A Haskell language frontend for the Nyar VM.

## Overview

`rusty-haskell` brings pure functional Haskell to the Nyar VM for writing concise, correct, and high-performance code with algebraic effects and multi-tier JIT.

## Features

- **Purely functional**: lazy evaluation, first-class functions, pattern matching.
- **Algebraic effects**: monads and effect systems mapped to Nyar algebraic effects and delimited continuations.
- **Nyar ecosystem integration**:
  - **`nyar-jit`**: high-performance execution of hot functional paths.
  - **`nyar-gc`**: precise memory management for closures and complex structures.
  - **`nyar-aot`**: E-Graph optimization for pure function transformations.
- **Strong typing**: advanced type features such as HKT and type classes mapped to Nyar internal representation.

## Supported constructs

- **Core syntax**: `let`, `module`, `data`, and `type` declarations.
- **Functions**: anonymous functions, currying, partial application.
- **Pattern matching**: exhaustive matching on data types.
- **Type classes**: (in progress) mapped to the Nyar trait system.

## Getting started

### Via Nyar CLI

```bash
nyar run script.hs
```

### As a library

```rust
use rusty_haskell::RustyHaskellFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyHaskellFrontend::new();
let ast = frontend.parse("square x = x * x").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
