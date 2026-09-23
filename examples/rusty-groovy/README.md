# rusty-groovy

A Groovy language frontend for the Nyar VM.

## Overview

`rusty-groovy` is a compiler frontend that brings the Groovy programming language to the Nyar VM. It provides a dynamic and flexible runtime for Groovy applications, mapping Groovy's features to Nyar's native execution engine.

## Features

- **Dynamic & Flexible**: Full support for Groovy's dynamic syntax and functional programming features.
- **Managed Object Model**: Maps Groovy classes and traits to Nyar's native object system.
- **JIT Optimization**: Optimized execution of hot Groovy code paths via `nyar-jit`.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run main.groovy
```

### Usage as a Library

```rust
use rusty_groovy::RustyGroovyFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyGroovyFrontend::new();
let ast = frontend.parse("println 'Hello from Groovy on Nyar!'").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
