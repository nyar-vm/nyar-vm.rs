# rusty-php

A PHP language frontend for the Nyar VM.

## Overview

`rusty-php` brings PHP, the web ecosystem workhorse, to the Nyar VM, running PHP applications on a modern JIT runtime with algebraic effects for request handling and GC for long-lived processes.

## Features

- **Web performance**: optimized for typical PHP request–response cycles.
- **Nyar ecosystem integration**:
  - **`nyar-jit`**: multi-tier optimization from interpreter to Extreme for hot functions and scripts.
  - **`nyar-gc`**: high-performance mark-and-sweep for PHP objects and arrays.
  - **Algebraic effects**: exceptions, generators, and future async patterns.
- **Modern runtime**: brings PHP ecosystem benefits to a multi-language VM.

## Supported constructs

- **Core syntax**: `class`, `function`, `namespace`, `use`.
- **Control flow**: `if`, `switch`, `for`, `foreach`, `while`.
- **Data types**: dynamic typing, associative arrays, and objects.
- **Web features**: basic support for superglobals (`$_GET`, `$_POST`) and output buffering.

## Getting started

### Via Nyar CLI

```bash
nyar run index.php
```

### As a library

```rust
use rusty_php::RustyPhpFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyPhpFrontend::new();
let ast = frontend.parse("<?php echo 'Hello from Nyar!'; ?>").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
