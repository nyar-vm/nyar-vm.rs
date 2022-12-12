# rusty-fortran

A Fortran language frontend for the Nyar VM.

## Overview

`rusty-fortran` is a compiler frontend that brings the power of Fortran—the original high-performance computing language—to the Nyar VM ecosystem. It allows scientific and engineering applications to leverage modern VM features like E-Graph based optimization, multi-tier JIT, and advanced garbage collection while maintaining compatibility with standard Fortran constructs.

## Features

- **Standard Fortran Support**: Targets common Fortran standards (F90/F95/F2003) for scientific computing.
- **High-Performance Math**: Maps Fortran's array operations and intrinsic math functions to Nyar's optimized Gaia IR instructions.
- **Nyar Ecosystem Integration**:
  - **`nyar-jit`**: Optimizes hot computational loops into native machine code.
  - **`nyar-gc`**: Provides automatic memory management for complex data structures.
  - **Algebraic Effects**: Can be used to implement advanced error handling or scientific data flows.
- **Modern Optimization**: Leverages `nyar-aot` for E-Graph based saturation optimization of numeric kernels.

## Supported Constructs

- **Program Structure**: `program`, `module`, `subroutine`, `function`.
- **Data Types**: `integer`, `real`, `complex`, `logical`, `character`.
- **Control Flow**: `if`, `do` loops, `select case`.
- **Array Handling**: Support for multidimensional arrays and array slicing (lower-unified implementation).

## Getting Started

### Usage via Nyar CLI

```bash
yar run calculations.f90
```

### Usage as a Library

```rust
use rusty_fortran::RustyFortranFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyFortranFrontend::new();
let ast = frontend.parse("program hello\nprint *, 'Hello'\nend program").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
