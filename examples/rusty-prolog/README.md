# rusty-prolog

A Prolog language frontend for the Nyar VM.

## Overview

`rusty-prolog` is a compiler frontend that brings logic programming to the Nyar VM ecosystem. It maps Prolog's core concepts—unification, backtracking, and logical variables—to Nyar's native algebraic effects and high-performance runtime, enabling the integration of logic-based reasoning into modern applications.

## Features

- **Logical Reasoning**: Full support for Horn clause logic, unification, and depth-first search.
- **Algebraic Effects Integration**: Implements Prolog's backtracking and non-deterministic execution using Nyar's native delimited continuations and effect handlers.
- **Nyar Ecosystem Integration**:
  - **`nyar-jit`**: Optimizes hot unification paths and recursive predicate calls.
  - **`nyar-gc`**: Automatically manages the lifecycle of logical variables and choice points.
  - **`nyar-aot`**: Supports E-Graph based optimization for logical goal transformations.
- **Interoperability**: Seamlessly call Prolog predicates from other Nyar languages like Python or Rust.

## Supported Constructs

- **Core Syntax**: Facts, rules, and queries.
- **Unification**: Robust implementation of Prolog's unification algorithm.
- **Control**: Support for the cut (`!`) operator, negation as failure, and logical disjunction.
- **Builtins**: Standard Prolog predicates for arithmetic, list manipulation, and I/O.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run knowledge_base.pl
```

### Usage as a Library

```rust
use rusty_prolog::RustyPrologFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyPrologFrontend::new();
let ast = frontend.parse("mortal(X) :- human(X). human(socrates).").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
