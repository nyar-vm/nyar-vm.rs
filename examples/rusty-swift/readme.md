# rusty-swift

A Swift language frontend for the Nyar VM.

## Overview

`rusty-swift` brings Swift, focused on safety, performance, and modern syntax, to the Nyar VM for developing modern applications in a managed environment with JIT, GC, and algebraic effects.

## Features

- **Safe and fast**: Swift's safety-first philosophy mapped to the Nyar managed runtime and Gaia IR.
- **Modern object model**: classes, structs, enums, protocols.
- **Nyar ecosystem integration**:
  - **`nyar-jit`**: multi-tier optimization of hot methods and closures.
  - **`nyar-gc`**: precise GC (mapping Swift ARC semantics where appropriate).
  - **Algebraic effects**: `async/await`, `try/catch`, and future concurrency models.
- **Interop**: design reserved for bridging with other Nyar languages such as Rust and future Objective-C.

## Supported constructs

- **Core syntax**: `func`, `var`, `let`, `class`, `struct`, `enum`, `protocol`.
- **Control flow**: `if`, `guard`, `switch`, `for-in`, `while`.
- **Optionals**: optional types and optional chaining.
- **Generics**: Swift generic programming features.

## Getting started

### Via Nyar CLI

```bash
nyar run application.swift
```

### As a library

```rust
use rusty_swift::RustySwiftFrontend;
use nyar_types::NyarFrontend;

let frontend = RustySwiftFrontend::new();
let ast = frontend.parse("func square(_ x: Int) -> Int { return x * x }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
