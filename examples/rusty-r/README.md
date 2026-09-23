# rusty-r

An R language frontend for the Nyar VM.

## Overview

`rusty-r` is a compiler frontend that brings the R programming language—the standard for statistical computing and data analysis—to the Nyar VM ecosystem. It allows data scientists to leverage Nyar's high-performance JIT and advanced memory management while maintaining compatibility with R's unique syntax and data structures.

## Features

- **Statistical Computing**: Designed to support R's core statistical and graphical capabilities.
- **Vectorized Operations**: Maps R's vectorized arithmetic and functional patterns to Nyar's optimized Gaia IR instructions.
- **Nyar Ecosystem Integration**:
  - **`nyar-jit`**: Optimizes hot computational loops and data processing pipelines.
  - **`nyar-gc`**: Provides automatic, precise memory management for large datasets and complex objects.
  - **`nyar-aot`**: Supports Ahead-of-Time optimization of R scripts using E-Graph based saturation.
- **Modern Performance**: Brings the benefits of a multi-tier VM (interpreter + JIT) to the R ecosystem.

## Supported Constructs

- **Core Syntax**: Assignments, function definitions, control flow (`if`, `for`, `while`).
- **Data Types**: Vectors (numeric, character, logical), lists, and data frames.
- **Functional Programming**: Support for closures, higher-order functions, and R's unique scoping rules.
- **Builtins**: Integrated with Nyar's FFI to provide standard R statistical functions.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run analysis.r
```

### Usage as a Library

```rust
use rusty_r::RustyRFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyRFrontend::new();
let ast = frontend.parse("sum_squares <- function(x) sum(x^2)").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
