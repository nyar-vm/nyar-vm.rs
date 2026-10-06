# rusty-nix

A Nix expression language frontend for the Nyar VM.

## Overview

`rusty-nix` brings the Nix expression language to the Nyar VM for evaluation in a high-performance managed environment, with algebraic effects for sandboxing and lazy evaluation.

## Features

- **Lazy evaluation**: mapped to Nyar thunks and on-demand execution.
- **Purely functional**: immutable data structures and side-effect-free evaluation.
- **Nyar ecosystem integration**:
  - **`nyar-jit`**: optimization of hot evaluation paths and function transformations.
  - **`nyar-gc`**: efficient collection of many short-lived small objects in Nix evaluation.
  - **Algebraic effects**: sandboxing and resource management.
- **Reproducible evaluation**: preserves Nix reproducibility guarantees within the Nyar runtime.

## Supported constructs

- **Core syntax**: `let`, `with`, `if-then-else`.
- **Functions**: lambdas, attribute-set patterns, partial application.
- **Data structures**: lists, attribute sets (recursive and non-recursive), interpolated strings.
- **Builtins**: standard Nix built-in functions via Nyar FFI.

## Getting started

### Via Nyar CLI

```bash
nyar run expression.nix
```

### As a library

```rust
use rusty_nix::RustyNixFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyNixFrontend::new();
let result = frontend.parse("{ a = 1; b = 2; }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
