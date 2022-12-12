# rusty-elixir

An Elixir language frontend for the Nyar VM.

## Overview

`rusty-elixir` is a compiler frontend that brings the Elixir programming language to the Nyar VM ecosystem. It focuses on Elixir's strengths in functional programming and concurrency, mapping Elixir's processes and message passing to Nyar's native task system and algebraic effects.

## Features

- **Functional Paradigm**: Full support for Elixir's immutable data structures and pattern matching.
- **Concurrency & Fault Tolerance**: Maps Elixir's process model to Nyar's native lightweight tasks and actor-based primitives.
- **Nyar Ecosystem Integration**:
  - **`nyar-jit`**: Provides high-performance execution of hot functional code paths and recursions.
  - **`nyar-gc`**: Automatically manages the lifecycle of atoms, maps, and other managed Elixir types.
  - **Algebraic Effects**: Uses Nyar's native effects to implement Elixir's control flow, error handling, and concurrency patterns.
- **Metaprogramming**: Integrated with Nyar's macro system to support Elixir's powerful `quote` and `unquote` capabilities.

## Supported Constructs

- **Core Syntax**: `defmodule`, `def`, `fn`, `quote`, `unquote`.
- **Pattern Matching**: Robust support for complex pattern matching in function heads and `case` statements.
- **Data Types**: Atoms, Lists, Maps, Tuples, and Binaries.
- **Piping**: Native support for the `|>` operator.
- **Protocols**: Implementation of Elixir protocols via Nyar's witness tables.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run script.ex
```

### Usage as a Library

```rust
use rusty_elixir::RustyElixirFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyElixirFrontend::new();
let ast = frontend.parse("defmodule Hello do def world do IO.puts \"Hello from Elixir!\" end end").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
