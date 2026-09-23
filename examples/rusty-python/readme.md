# rusty-python

A Python language frontend for the Nyar VM.

## Overview

`rusty-python` is a compiler frontend that allows Python source code to be executed on the Nyar VM. It leverages the `oak-python` parser to generate an Abstract Syntax Tree (AST), which is then lowered into Gaia Intermediate Representation (Gaia IR) for high-performance execution.

## Features

- **Python 3 Compatibility**: Support for a significant subset of Python 3 syntax and features.
- **Efficient Variable Scoping**: Correct handling of local, global, and nonlocal scopes.
- **Object Model Mapping**: Maps Python's dynamic object model (attributes, methods) to the Nyar VM's object system.
- **Nyar Integration**: Seamlessly compiles to Gaia IR, enabling JIT optimization via `nyar-jit`.
- **Interoperability**: Ability to call functions and use objects defined in other Nyar-supported languages.

## Supported Constructs

- **Control Flow**: `if`, `for`, `while`, `try-except`, `with`.
- **Data Structures**: Lists, Dictionaries, Sets, Tuples.
- **Functions**: Support for closures, decorators, and generator functions (via algebraic effects).
- **Classes**: Full support for class definitions, inheritance, and magic methods.
- **Assignments**: Multiple assignments, augmented assignments (`+=`, `-=`, etc.).

## Getting Started

### Usage via Nyar CLI

```bash
nyar run example.py
```

### Usage as a Library

```rust
use rusty_python::RustyPythonFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyPythonFrontend::new();
let ast = frontend.parse("print('Hello from Nyar!')").unwrap();
// Lower and execute via NyarVM
```

## Status

`rusty-python` is currently in active development. While it supports many core Python features, some parts of the standard library and advanced language features (like metaclasses) are still being implemented.

## License

Licensed under MIT OR Apache-2.0.
