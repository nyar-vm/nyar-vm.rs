# rusty-tcl

A TCL language frontend for the Nyar VM.

## Overview

`rusty-tcl` is a compiler frontend that brings the TCL (Tool Command Language) to the Nyar VM ecosystem. Known for its simplicity and extensibility, TCL finds a new home in Nyar, where it can be used for everything from simple scripting to complex system orchestration, all running on a high-performance JIT-optimized runtime.

## Features

- **Everything is a String**: Maintains TCL's core philosophy while efficiently mapping strings to Nyar's internal representations (including NaN-boxed values where possible).
- **Command-Centric Design**: Implements TCL's command evaluation model using Nyar's flexible virtual call and effect system.
- **Nyar Ecosystem Integration**:
  - **`nyar-jit`**: Optimizes hot command sequences and script loops.
  - **`nyar-gc`**: Automatically manages memory for TCL variables and dynamic strings.
  - **Algebraic Effects**: Used to implement TCL's control flow commands and error handling patterns.
- **Extensible Architecture**: Easily add new TCL commands by bridging to Nyar's FFI or other languages.

## Supported Constructs

- **Core Syntax**: Command substitution, variable substitution, backslash substitution.
- **Variables**: Support for global and local variables via the `set` command.
- **Control Flow**: Basic support for `if`, `while`, and custom control structures.
- **Procedures**: Support for defining and calling `proc` with local scoping.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run script.tcl
```

### Usage as a Library

```rust
use rusty_tcl::RustyTclFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyTclFrontend::new();
let ast = frontend.parse("set x 10; puts $x").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
