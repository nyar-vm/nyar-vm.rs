# rusty-dart

A Dart language frontend for the Nyar VM.

## Overview

`rusty-dart` is a compiler frontend that brings the Dart programming language to the Nyar VM ecosystem. It provides a high-performance runtime for Dart applications, leveraging Nyar's advanced JIT compilation, garbage collection, and algebraic effects for both mobile and server-side scenarios.

## Features

- **Modern Dart Support**: Targeted at modern Dart specifications, including null safety and sound typing.
- **Efficient Object System**: Maps Dart's class-based object model, mixins, and extensions to Nyar's native object system.
- **Nyar Ecosystem Integration**:
  - **`nyar-jit`**: Provides multi-tier optimization (Baseline to Extreme) for hot Dart code.
  - **`nyar-gc`**: Leverages Nyar's precise garbage collector for high-performance memory management.
  - **Algebraic Effects**: Maps Dart's `async/await`, `Future`, and `Stream` abstractions to Nyar's native effects and continuations.
- **Hot Reload Capability**: Designed to support future hot-reload features within the Nyar development environment.

## Supported Constructs

- **Core Syntax**: `class`, `mixin`, `extension`, `enum`.
- **Functions**: Support for closures, arrow functions, and optional parameters.
- **Collections**: Integrated support for Dart Lists, Sets, and Maps.
- **Async Programming**: Full support for `async`, `await`, and `yield`.
- **Type System**: Support for Dart's sound type system and null safety checks.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run main.dart
```

### Usage as a Library

```rust
use rusty_dart::RustyDartFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyDartFrontend::new();
let ast = frontend.parse("void main() { print('Hello from Dart on Nyar!'); }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
