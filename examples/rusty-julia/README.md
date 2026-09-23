# rusty-julia

A Julia language frontend for the Nyar VM.

## Overview

`rusty-julia` is a compiler frontend that brings the Julia programming language—known for its combination of high-level ease and low-level performance—to the Nyar VM ecosystem. It targets scientific computing, data analysis, and general-purpose programming by mapping Julia's multiple dispatch and dynamic type system to Nyar's advanced runtime.

## Features

- **Multiple Dispatch**: Maps Julia's core multiple dispatch paradigm to Nyar's native virtual call and dynamic dispatch mechanisms.
- **Just-In-Time Specialization**: Leverages `nyar-jit` to perform type-specialized compilation for hot code paths, similar to Julia's own LLVM-based JIT.
- **Nyar Ecosystem Integration**:
  - **`nyar-jit`**: Provides multi-tier optimization (Baseline, Optimizing, Extreme) for numeric kernels.
  - **`nyar-gc`**: Manages complex object graphs and arrays with high-performance mark-and-sweep.
  - **`nyar-aot`**: Supports Ahead-of-Time optimization of static Julia modules using E-Graph saturation.
- **Metaprogramming**: Support for Julia's powerful macro and expression manipulation system, integrated with Nyar's native metaprogramming primitives.

## Supported Constructs

- **Core Syntax**: `function`, `struct`, `module`, `macro`.
- **Control Flow**: `if`, `while`, `for`, `try/catch`.
- **Types**: Support for parametric types and abstract type hierarchies.
- **Arrays**: Native integration with Nyar's array representation for high-performance linear algebra.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run analysis.jl
```

### Usage as a Library

```rust
use rusty_julia::RustyJuliaFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyJuliaFrontend::new();
let ast = frontend.parse("f(x) = x^2 + 2x + 1").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
