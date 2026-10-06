# rusty-c

A C language frontend for the Nyar VM.

## Overview

`rusty-c` runs C source on the Nyar VM, bridging procedural C with a managed runtime (GC + JIT) instead of a raw native-only process model.

## Features

- **C99/C11 subset**: standard syntax and common extensions.
- **Managed execution**: heap allocations can route through GC-aware allocators.
- **Pointer model**: VM-enforced pointer rules to reduce common memory errors.
- **Nyar integration**: lowers procedural C to efficient unified IR.
- **JIT**: hot functions compiled by `nyar-jit`.

## Supported constructs

- **Core syntax**: `struct`, `union`, `enum`, `typedef`.
- **Control flow**: `if`, `switch`, `for`, `while`, `do-while`, `goto`.
- **Functions**: recursion, function pointers, variadics.
- **Pointers and arrays**: multi-dimensional arrays and pointer arithmetic (within supported subset).
- **Preprocessor**: standard C preprocessing (integrated or external pass).

## Getting started

### Via Nyar CLI

```bash
nyar run main.c
```

### As a library

```rust
use rusty_c::RustyCFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyCFrontend::new();
let ast = frontend.parse("int main() { return 42; }").unwrap();
```

## License

Licensed under MIT OR Apache-2.0.
