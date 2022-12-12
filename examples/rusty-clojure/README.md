# rusty-clojure

A Clojure language frontend for the Nyar VM.

## Overview

`rusty-clojure` is a compiler frontend that brings the Clojure programming language to the Nyar VM. It provides a functional and concurrent runtime for Clojure applications, mapping Clojure's Lisp-style features to Nyar's native execution engine and algebraic effects.

## Features

- **Functional & Concurrent**: Support for Clojure's immutable data structures and functional programming model.
- **Lisp-style Syntax**: Maps Clojure's S-expression based syntax to Nyar's internal representation.
- **JIT Optimization**: Optimized execution of hot Clojure code paths via `nyar-jit`.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run main.clj
```

### Usage as a Library

```rust
use rusty_clojure::RustyClojureFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyClojureFrontend::new();
let ast = frontend.parse("(println \"Hello from Clojure on Nyar!\")").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
