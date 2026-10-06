# rusty-tcl

A Tcl language frontend for the Nyar VM.

## Overview

`rusty-tcl` brings Tcl (Tool Command Language), simple and extensible, to the Nyar VM for everything from simple scripts to complex system orchestration, running on a JIT-optimized high-performance runtime.

## Features

- **Everything is a string**: preserves Tcl's core philosophy while efficiently mapping to Nyar internal representation (including NaN-boxing).
- **Command-centric design**: Tcl command evaluation model implemented via Nyar virtual dispatch and effect system.
- **Nyar ecosystem integration**:
  - **`nyar-jit`**: optimization of hot command sequences and script loops.
  - **`nyar-gc`**: automatic management of Tcl variables and dynamic strings.
  - **Algebraic effects**: Tcl control-flow commands and error-handling patterns.
- **Extensible architecture**: new Tcl commands via Nyar FFI or bridges to other languages.

## Supported constructs

- **Core syntax**: command substitution, variable substitution, backslash substitution.
- **Variables**: global and local variables via `set`.
- **Control flow**: basic support for `if`, `while`, and custom control structures.
- **Procedures**: `proc` definition and invocation with local scope.

## Getting started

### Via Nyar CLI

```bash
nyar run script.tcl
```

### As a library

```rust
use rusty_tcl::RustyTclFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyTclFrontend::new();
let ast = frontend.parse("set x 10; puts $x").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
