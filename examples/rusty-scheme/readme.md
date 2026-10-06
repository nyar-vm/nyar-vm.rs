# rusty-scheme

A Scheme language frontend for the Nyar VM.

## Overview

`rusty-scheme` brings the minimalism of classic Lisp dialect Scheme to the Nyar VM, mapping first-class continuations, proper tail calls, and hygienic macros to the Nyar native runtime.

## Features

- **Minimal Lisp**: oriented toward R5RS/R6RS/R7RS core specifications.
- **First-class continuations**: `call-with-current-continuation` via Nyar algebraic effects and delimited continuations.
- **Nyar ecosystem integration**:
  - **`nyar-jit`**: optimization of hot recursion and function transformations.
  - **`nyar-gc`**: lifecycle management for pairs, closures, and symbols.
  - **Proper tail calls**: tail-call optimization guaranteed by the Nyar VM core.
- **Hygienic macros**: integrated with the Nyar macro system.

## Supported constructs

- **Core syntax**: `define`, `lambda`, `if`, `set!`, `quote`.
- **Data types**: symbols, lists (pairs), numbers, booleans, characters.
- **Control flow**: `cond`, `case`, `do`, `begin`.
- **Continuations**: `call/cc` and related control abstractions.

## Getting started

### Via Nyar CLI

```bash
nyar run script.scm
```

### As a library

```rust
use rusty_scheme::RustySchemeFrontend;
use nyar_types::NyarFrontend;

let frontend = RustySchemeFrontend::new();
let result = frontend.parse("(define (factorial n) (if (= n 0) 1 (* n (factorial (- n 1)))))").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
