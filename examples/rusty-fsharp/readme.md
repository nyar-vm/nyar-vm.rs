# rusty-fsharp

An F# language frontend for the Nyar VM.

## Overview

`rusty-fsharp` brings the functional-first F# to the Nyar VM for writing concise, correct, and high-performance code with algebraic effects and multi-tier JIT.

## Features

- **Functional-first**: immutability, first-class functions, pattern matching.
- **Algebraic effects**: computation expressions (e.g. `async { ... }`, `seq { ... }`) mapped to Nyar effects and delimited continuations.
- **Nyar ecosystem integration**:
  - **`nyar-jit`**: high-performance execution of hot functional paths.
  - **`nyar-gc`**: precise memory management for closures and complex structures.
  - **`nyar-aot`**: E-Graph optimization for pure function transformations.
- **Type safety**: F#'s strong type system mapped to Nyar internal type representation.

## Supported constructs

- **Core syntax**: `let`, `module`, `type` (records, unions).
- **Functions**: anonymous functions, currying, partial application.
- **Pattern matching**: exhaustive matching on records and discriminated unions.
- **Computation expressions**: mapped to Nyar effect handlers.

## Getting started

### Via Nyar CLI

```bash
nyar run script.fs
```

### As a library

```rust
use rusty_fsharp::RustyFSharpFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyFSharpFrontend::new();
let ast = frontend.parse("let square x = x * x").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
