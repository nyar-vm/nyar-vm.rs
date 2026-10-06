# rusty-julia

A Julia language frontend for the Nyar VM.

## Overview

`rusty-julia` brings Julia, combining high-level ease of use with low-level performance, to the Nyar VM for scientific computing and data analysis, mapping multiple dispatch and dynamic typing to the Nyar advanced runtime.

## Features

- **Multiple dispatch**: mapped to Nyar virtual dispatch and dynamic dispatch.
- **JIT specialization**: `nyar-jit` type-specialized compilation of hot paths, similar to Julia's LLVM JIT.
- **Nyar ecosystem integration**:
  - **`nyar-jit`**: multi-tier optimization of numerical kernels.
  - **`nyar-gc`**: high-performance mark-and-sweep for complex object graphs and arrays.
  - **`nyar-aot`**: E-Graph ahead-of-time optimization for static Julia modules.
- **Metaprogramming**: macros and expression manipulation integrated with Nyar metaprogramming primitives.

## Supported constructs

- **Core syntax**: `function`, `struct`, `module`, `macro`.
- **Control flow**: `if`, `while`, `for`, `try/catch`.
- **Types**: parametric types and abstract type hierarchies.
- **Arrays**: integrated with Nyar array representation for linear algebra support.

## Getting started

### Via Nyar CLI

```bash
nyar run analysis.jl
```

### As a library

```rust
use rusty_julia::RustyJuliaFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyJuliaFrontend::new();
let ast = frontend.parse("f(x) = x^2 + 2x + 1").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
