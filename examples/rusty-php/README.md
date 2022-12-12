# rusty-php

A PHP language frontend for the Nyar VM.

## Overview

`rusty-php` is a compiler frontend that brings the PHP language—the backbone of the web—to the Nyar VM ecosystem. It allows PHP applications to run on a modern, high-performance JIT-optimized runtime, providing benefits like algebraic effects for request handling and advanced garbage collection for long-running processes.

## Features

- **Web-Centric Performance**: Optimized for the typical request-response cycle of PHP applications.
- **Nyar Ecosystem Integration**:
  - **`nyar-jit`**: Provides multi-tier optimization (Interpreter to Extreme) for hot PHP functions and scripts.
  - **`nyar-gc`**: Automatically manages memory for PHP objects and arrays with a high-performance mark-and-sweep collector.
  - **Algebraic Effects**: Used to implement PHP's exception handling, generator functions, and future async patterns.
- **Modern Runtime**: Brings the advantages of a modern, multi-language VM to the PHP ecosystem.

## Supported Constructs

- **Core Syntax**: `class`, `function`, `namespace`, `use`.
- **Control Flow**: `if`, `switch`, `for`, `foreach`, `while`.
- **Data Types**: Support for PHP's dynamic types, including associative arrays and objects.
- **Web Features**: Basic support for superglobals (`$_GET`, `$_POST`) and output buffering.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run index.php
```

### Usage as a Library

```rust
use rusty_php::RustyPhpFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyPhpFrontend::new();
let ast = frontend.parse("<?php echo 'Hello from Nyar!'; ?>").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
