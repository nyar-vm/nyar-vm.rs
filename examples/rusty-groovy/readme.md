# rusty-groovy

A Groovy language frontend for the Nyar VM.

## Overview

`rusty-groovy` brings Groovy to the Nyar VM, providing a runtime for dynamic, flexible Groovy applications by mapping Groovy features to the Nyar native execution engine.

## Features

- **Dynamic and flexible**: Groovy dynamic syntax and functional features.
- **Managed object model**: Groovy classes and traits mapped to the Nyar object system.
- **JIT optimization**: hot Groovy paths optimized via `nyar-jit`.

## Getting started

### Via Nyar CLI

```bash
nyar run main.groovy
```

### As a library

```rust
use rusty_groovy::RustyGroovyFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyGroovyFrontend::new();
let ast = frontend.parse("println 'Hello from Groovy on Nyar!'").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
