# rusty-fortran

A Fortran language frontend for the Nyar VM.

## Overview

`rusty-fortran` brings Fortran, the classic language for scientific computing, to the Nyar VM, enabling science and engineering applications to use E-Graph optimization, multi-tier JIT, and advanced GC while maintaining compatibility with standard Fortran constructs.

## Features

- **Standard Fortran**: oriented toward F90/F95/F2003 and other standards common in scientific computing.
- **High-performance math**: array operations and built-in math functions mapped to optimized Gaia IR.
- **Nyar ecosystem integration**:
  - **`nyar-jit`**: hot computation loops compiled to native code.
  - **`nyar-gc`**: automatic memory management for complex data structures.
  - **Algebraic effects**: scientific data flows and advanced error handling.
- **Modern optimization**: E-Graph saturation optimization of numerical kernels via `nyar-aot`.

## Supported constructs

- **Program structure**: `program`, `module`, `subroutine`, `function`.
- **Data types**: `integer`, `real`, `complex`, `logical`, `character`.
- **Control flow**: `if`, `do`, `select case`.
- **Arrays**: multi-dimensional arrays and slices (lower-unified implementation).

## Getting started

### Via Nyar CLI

```bash
nyar run calculations.f90
```

### As a library

```rust
use rusty_fortran::RustyFortranFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyFortranFrontend::new();
let ast = frontend.parse("program hello\nprint *, 'Hello'\nend program").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
