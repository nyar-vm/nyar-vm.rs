# rusty-prolog

A Prolog language frontend for the Nyar VM.

## Overview

`rusty-prolog` brings logic programming to the Nyar VM, mapping unification, backtracking, and logic variables to Nyar algebraic effects and a high-performance runtime for integrating logical reasoning into modern applications.

## Features

- **Logical reasoning**: Horn clauses, unification, and depth-first search.
- **Algebraic effects**: backtracking and nondeterministic execution via delimited continuations and effect handlers.
- **Nyar ecosystem integration**:
  - **`nyar-jit`**: optimization of hot unification paths and recursive predicates.
  - **`nyar-gc`**: lifecycle management for logic variables and choice points.
  - **`nyar-aot`**: E-Graph optimization of logic goals.
- **Interop**: Prolog predicates callable from other Nyar languages such as Python and Rust.

## Supported constructs

- **Core syntax**: facts, rules, and queries.
- **Unification**: Prolog unification algorithm.
- **Control**: cut `!`, negation as failure, logical or.
- **Builtins**: standard predicates for arithmetic, lists, I/O, and more.

## Getting started

### Via Nyar CLI

```bash
nyar run knowledge_base.pl
```

### As a library

```rust
use rusty_prolog::RustyPrologFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyPrologFrontend::new();
let ast = frontend.parse("mortal(X) :- human(X). human(socrates).").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
