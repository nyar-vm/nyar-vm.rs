# rusty-go

A Go language frontend for the Nyar VM.

## Overview

`rusty-go` is a compiler frontend that brings the Go programming language to the Nyar VM. It focuses on Go's strengths—simplicity and concurrency—by mapping Go's goroutines and channels to Nyar's native concurrency primitives and algebraic effects.

## Features

- **Go-style Concurrency**: Goroutines and channels implemented via Nyar's lightweight task system.
- **Efficient Memory Management**: Integrated with `nyar-gc` for robust garbage collection.
- **Fast Startup**: Optimized for quick compilation and execution, ideal for microservices and scripting.
- **Nyar Integration**: Seamlessly compiles to Gaia IR for JIT-optimized performance.
- **Static Typing**: Maintains Go's strong, static type system during the lowering process.

## Supported Constructs

- **Core Syntax**: `package`, `import`, `func`, `var`, `type`.
- **Concurrency**: `go` keyword, `chan`, `select`.
- **Control Flow**: `if`, `for`, `switch`, `defer`.
- **Data Types**: Slices, Maps, Structs, Interfaces.
- **Pointers**: Safe pointer support consistent with Go's memory model.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run main.go
```

### Usage as a Library

```rust
use rusty_go::frontend::RustyGoFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyGoFrontend::new();
let ast = frontend.parse("package main\nfunc main() { println(\"Hello from Go!\") }").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
