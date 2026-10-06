# rusty-clojure

A Clojure language frontend for the Nyar VM.

## Overview

`rusty-clojure` brings Clojure to the Nyar VM, providing a runtime for functional and concurrent Clojure programs by mapping Lisp-style features to the Nyar execution engine and algebraic effects.

## Features

- **Functional and concurrent**: immutable data structures and a functional programming model.
- **Lisp syntax**: S-expressions mapped to Nyar internal representation.
- **JIT optimization**: hot paths optimized via `nyar-jit`.

## Getting started

### Via Nyar CLI

```bash
nyar run main.clj
```

### As a library

```rust
use rusty_clojure::RustyClojureFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyClojureFrontend::new();
let ast = frontend.parse("(println \"Hello from Clojure on Nyar!\")").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
