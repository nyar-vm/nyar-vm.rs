# rusty-elixir

An Elixir language frontend for the Nyar VM.

## Overview

`rusty-elixir` brings Elixir to the Nyar VM, mapping the process model and message passing to Nyar lightweight tasks and algebraic effects to leverage functional and concurrent strengths.

## Features

- **Functional paradigm**: immutable data structures and pattern matching.
- **Concurrency and fault tolerance**: Elixir processes mapped to Nyar actor-style primitives.
- **Nyar ecosystem integration**:
  - **`nyar-jit`**: optimization of hot functional paths and recursion.
  - **`nyar-gc`**: lifecycle management for managed types such as atoms and maps.
  - **Algebraic effects**: control flow, error handling, and concurrency patterns.
- **Metaprogramming**: `quote`/`unquote` integrated with the Nyar macro system.

## Supported constructs

- **Core syntax**: `defmodule`, `def`, `fn`, `quote`, `unquote`.
- **Pattern matching**: complex patterns in function heads and `case`.
- **Data types**: atoms, lists, maps, tuples, binaries.
- **Pipe operator**: `|>`.
- **Protocols**: Elixir protocols via witness tables.

## Getting started

### Via Nyar CLI

```bash
nyar run script.ex
```

### As a library

```rust
use rusty_elixir::RustyElixirFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyElixirFrontend::new();
let ast = frontend.parse("defmodule Hello do def world do IO.puts \"Hello from Elixir!\" end end").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
