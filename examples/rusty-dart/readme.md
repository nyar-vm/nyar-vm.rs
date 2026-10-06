# rusty-dart

A Dart language frontend for the Nyar VM.

## Overview

`rusty-dart` brings Dart to the Nyar VM for mobile and server-side scenarios, providing a high-performance runtime with JIT, GC, and algebraic effects.

## Features

- **Modern Dart**: modern specifications including null safety and sound typing.
- **Efficient object system**: classes, mixins, and extensions mapped to the Nyar object system.
- **Nyar ecosystem integration**:
  - **`nyar-jit`**: multi-tier optimization from Baseline to Extreme.
  - **`nyar-gc`**: precise GC for Dart objects.
  - **Algebraic effects**: `async`/`await`, `Future`, and `Stream` mapped to Nyar effects and continuations.
- **Hot reload**: design space reserved for hot reload within the Nyar development environment.

## Supported constructs

- **Core syntax**: `class`, `mixin`, `extension`, `enum`.
- **Functions**: closures, arrow functions, optional parameters.
- **Collections**: List, Set, Map.
- **Async**: `async`, `await`, `yield`.
- **Type system**: sound typing and null-safety checks.

## Getting started

### Via Nyar CLI

```bash
nyar run main.dart
```

### As a library

```rust
use rusty_dart::RustyDartFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyDartFrontend::new();
let ast = frontend.parse("void main() { print('Hello from Dart on Nyar!'); }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
