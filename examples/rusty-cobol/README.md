# rusty-cobol

A COBOL language frontend for the Nyar VM.

## Overview

`rusty-cobol` is a specialized compiler frontend that brings the venerable COBOL language to the modern Nyar VM ecosystem. It allows legacy business logic and data processing applications to run on a high-performance, JIT-optimized runtime with advanced garbage collection and concurrency support.

## Features

- **Standard COBOL Support**: Aims to support common COBOL dialects and standards (COBOL-85/2002).
- **Modern Execution Environment**: Runs COBOL code on the `nyar-vm`, providing benefits like automatic memory management (via `nyar-gc`) and multi-tier JIT optimization.
- **Data Division Mapping**: Maps COBOL's complex data structures and pictures to Nyar's native types and objects.
- **Nyar Ecosystem Integration**:
  - **`nyar-jit`**: Optimizes hot business logic paths and numeric processing.
  - **`nyar-gc`**: Automatically manages the lifecycle of managed COBOL records.
  - **`nyar-aot`**: Supports Ahead-of-Time optimization for batch processing modules.
- **Interoperability**: Seamlessly integrates with modern Nyar languages for hybrid application development.

## Supported Constructs

- **Divisions**: Identification, Environment, Data, and Procedure divisions.
- **Data Types**: Alphanumeric, Numeric (including fixed-point decimals), and Group items.
- **Control Flow**: `PERFORM`, `IF`, `EVALUATE`, `GO TO`.
- **File I/O**: Initial support for standard COBOL file handling operations.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run program.cbl
```

### Usage as a Library

```rust
use rusty_cobol::RustyCobolFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyCobolFrontend::new();
let ast = frontend.parse("IDENTIFICATION DIVISION. PROGRAM-ID. HELLO.").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
