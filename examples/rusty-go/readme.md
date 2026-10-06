# rusty-go

A Go language frontend for the Nyar VM.

## Overview

`rusty-go` brings Go to the Nyar VM, mapping goroutines and channels to Nyar lightweight tasks and algebraic effects.

## Features

- **Go-style concurrency**: goroutines and channels on Nyar's task system.
- **Managed memory**: integrates with `nyar-gc`.
- **Fast startup**: tuned for microservices and scripting workloads.
- **Nyar integration**: lowers to unified IR for JIT optimization.
- **Static typing**: preserves Go's static type discipline during lowering.

## Supported constructs

- **Core syntax**: `package`, `import`, `func`, `var`, `type`.
- **Concurrency**: `go`, `chan`, `select`.
- **Control flow**: `if`, `for`, `switch`, `defer`.
- **Data types**: slices, maps, structs, interfaces.
- **Pointers**: pointer support aligned with Go memory model expectations.

## Getting started

### Via Nyar CLI

```bash
nyar run main.go
```

### As a library

```rust
use rusty_go::frontend::RustyGoFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyGoFrontend::new();
let ast = frontend.parse("package main\nfunc main() { println(\"Hello from Go!\") }").unwrap();
```

## License

Licensed under MIT OR Apache-2.0.
