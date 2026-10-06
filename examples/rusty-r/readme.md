# rusty-r

An R language frontend for the Nyar VM.

## Overview

`rusty-r` brings R, the standard language for statistical computing, to the Nyar VM, enabling data science workflows to enjoy Nyar JIT and memory management while preserving R syntax and data structures.

## Features

- **Statistical computing**: oriented toward R's core statistical and graphics capabilities.
- **Vectorized operations**: vector arithmetic and functional patterns mapped to optimized Gaia IR.
- **Nyar ecosystem integration**:
  - **`nyar-jit`**: optimization of hot computation loops and data processing pipelines.
  - **`nyar-gc`**: precise memory management for large datasets and complex objects.
  - **`nyar-aot`**: E-Graph ahead-of-time optimization for R scripts.
- **Modern performance**: brings R ecosystem benefits to an interpreter + JIT multi-tier VM.

## Supported constructs

- **Core syntax**: assignment, function definitions, `if`/`for`/`while`.
- **Data types**: vectors (numeric, character, logical), lists, data frames.
- **Functional**: closures, higher-order functions, and R's unique scoping rules.
- **Builtins**: standard R statistical functions via Nyar FFI.

## Getting started

### Via Nyar CLI

```bash
nyar run analysis.r
```

### As a library

```rust
use rusty_r::RustyRFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyRFrontend::new();
let ast = frontend.parse("sum_squares <- function(x) sum(x^2)").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
