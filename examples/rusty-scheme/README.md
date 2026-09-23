# rusty-scheme

A Scheme language frontend for the Nyar VM.

## Overview

`rusty-scheme` is a compiler frontend that brings the elegance and minimalism of Scheme—a classic Lisp dialect—to the Nyar VM ecosystem. It maps Scheme's core concepts like first-class continuations, proper tail calls, and hygienic macros to Nyar's native runtime, providing a high-performance home for functional programming.

## Features

- **Minimalist Lisp Dialect**: Support for the core R5RS/R6RS/R7RS Scheme specifications.
- **First-Class Continuations**: Implements `call-with-current-continuation` using Nyar's native algebraic effects and delimited continuations.
- **Nyar Ecosystem Integration**:
  - **`nyar-jit`**: Optimizes hot recursive functions and functional transformations.
  - **`nyar-gc`**: Automatically manages the lifecycle of pairs, closures, and symbols.
  - **Proper Tail Calls**: Guaranteed tail-call optimization provided by the Nyar VM core.
- **Hygienic Macros**: Integrated with Nyar's macro system for safe and powerful code generation.

## Supported Constructs

- **Core Syntax**: `define`, `lambda`, `if`, `set!`, `quote`.
- **Data Types**: Symbols, lists (pairs), numbers, booleans, and characters.
- **Control Flow**: `cond`, `case`, `do`, `begin`.
- **Continuations**: Full support for `call/cc` and related control abstractions.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run script.scm
```

### Usage as a Library

```rust
use rusty_scheme::RustySchemeFrontend;
use nyar_types::NyarFrontend;

let frontend = RustySchemeFrontend::new();
let result = frontend.parse("(define (factorial n) (if (= n 0) 1 (* n (factorial (- n 1)))))").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
