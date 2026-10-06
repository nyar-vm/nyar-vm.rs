# rusty-python

A Python language frontend for the Nyar VM.

## Overview

`rusty-python` parses Python 3 source (via `oak-python`) into an AST, then lowers to unified IR for optimized execution on Nyar.

## Features

- **Python 3 subset**: common syntax and runtime features.
- **Scoping**: local, global, and nonlocal name rules.
- **Object model**: attributes and methods on Nyar VM objects.
- **Nyar integration**: IR output consumable by `nyar-jit`.
- **Interop**: call functions and objects defined in other Nyar frontends.

## Supported constructs

- **Control flow**: `if`, `for`, `while`, `try-except`, `with`.
- **Data structures**: list, dict, set, tuple.
- **Functions**: closures, decorators, generators (via algebraic effects).
- **Classes**: inheritance and dunder methods.
- **Assignment**: multiple and augmented assignment (`+=`, …).

## Getting started

### Via Nyar CLI

```bash
nyar run example.py
```

### As a library

```rust
use rusty_python::RustyPythonFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyPythonFrontend::new();
let ast = frontend.parse("print('Hello from Nyar!')").unwrap();
```

## Status

Active development: core syntax is largely covered; stdlib surface and metaclasses remain incomplete.

## License

Licensed under MIT OR Apache-2.0.
