# rusty-cobol

A COBOL language frontend for the Nyar VM.

## Overview

`rusty-cobol` brings COBOL business logic to the modern Nyar VM, enabling legacy data-processing applications to run on a high-performance runtime with JIT optimization, advanced GC, and concurrency support.

## Features

- **Standard COBOL**: oriented toward COBOL-85/2002 and other common dialects and standards.
- **Modern execution environment**: runs on `nyar-vm` with `nyar-gc` and multi-tier JIT.
- **Data Division mapping**: complex data structures such as PICTURE mapped to Nyar types and objects.
- **Nyar ecosystem integration**:
  - **`nyar-jit`**: optimization of hot business logic and numeric processing.
  - **`nyar-gc`**: managed lifecycle for COBOL records.
  - **`nyar-aot`**: ahead-of-time optimization for batch processing modules.
- **Interop**: mixed development with modern Nyar languages.

## Supported constructs

- **Divisions**: Identification, Environment, Data, Procedure.
- **Data types**: alphanumeric, numeric (including fixed-point decimal), group items.
- **Control flow**: `PERFORM`, `IF`, `EVALUATE`, `GO TO`.
- **File I/O**: initial support for standard COBOL file operations.

## Getting started

### Via Nyar CLI

```bash
nyar run program.cbl
```

### As a library

```rust
use rusty_cobol::RustyCobolFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyCobolFrontend::new();
let ast = frontend.parse("IDENTIFICATION DIVISION. PROGRAM-ID. HELLO.").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
