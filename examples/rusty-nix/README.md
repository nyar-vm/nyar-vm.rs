# rusty-nix

A Nix language frontend for the Nyar VM.

## Overview

`rusty-nix` is a compiler frontend that brings the Nix expression language to the Nyar VM ecosystem. It allows Nix expressions to be evaluated within a high-performance, managed environment, leveraging Nyar's advanced features like algebraic effects for sandboxing and lazy evaluation.

## Features

- **Lazy Evaluation**: Maps Nix's core lazy evaluation model to Nyar's native thunks and demand-driven execution.
- **Pure Functional Paradigm**: Robust support for immutable data structures and side-effect-free evaluation.
- **Nyar Ecosystem Integration**:
  - **`nyar-jit`**: Optimizes hot evaluation paths and functional transformations.
  - **`nyar-gc`**: Efficiently manages the lifecycle of thousands of small, short-lived objects common in Nix evaluations.
  - **Algebraic Effects**: Used to implement flexible sandboxing and resource management for Nix builds.
- **Reproducible Evaluation**: Designed to maintain Nix's core guarantee of reproducibility within the Nyar runtime.

## Supported Constructs

- **Core Syntax**: `let` expressions, `with` statements, `if-then-else`.
- **Functions**: Support for lambda expressions, attribute set patterns, and partial application.
- **Data Structures**: Lists, attribute sets (recursive and non-recursive), and strings with interpolation.
- **Builtins**: Integrated with Nyar's FFI system to provide standard Nix builtin functions.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run expression.nix
```

### Usage as a Library

```rust
use rusty_nix::RustyNixFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyNixFrontend::new();
let result = frontend.parse("{ a = 1; b = 2; }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
