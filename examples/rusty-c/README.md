# rusty-c

A C language frontend for the Nyar VM.

## Overview

`rusty-c` is a compiler frontend that allows C source code to be executed on the Nyar VM. It provides a bridge between low-level systems programming and Nyar's high-level managed environment, allowing C code to benefit from automatic memory management and JIT optimization.

## Features

- **C99/C11 Compatibility**: Support for standard C syntax and common extensions.
- **Managed Execution**: C code runs within the `nyar-vm` environment, with automatic garbage collection for heap-allocated memory (via custom `malloc`/`free` mappings).
- **Pointer Safety**: Implements a safe pointer model within the VM to prevent common C memory errors like buffer overflows.
- **Nyar Integration**: Translates C's procedural model into efficient Gaia IR.
- **JIT Optimization**: Hot C functions are automatically compiled to native machine code by `nyar-jit`.

## Supported Constructs

- **Core Syntax**: `struct`, `union`, `enum`, `typedef`.
- **Control Flow**: `if`, `switch`, `for`, `while`, `do-while`, `goto`.
- **Functions**: Support for recursion, function pointers, and variadic functions.
- **Pointers & Arrays**: Multi-dimensional arrays and complex pointer arithmetic.
- **Preprocessors**: Support for standard C preprocessor directives (via external preprocessor or integrated pass).

## Getting Started

### Usage via Nyar CLI

```bash
nyar run main.c
```

### Usage as a Library

```rust
use rusty_c::RustyCFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyCFrontend::new();
let ast = frontend.parse("int main() { return 42; }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
