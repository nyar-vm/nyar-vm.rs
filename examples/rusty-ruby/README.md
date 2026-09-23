# rusty-ruby

A Ruby language frontend for the Nyar VM.

## Overview

`rusty-ruby` is a high-performance Ruby frontend for the Nyar VM ecosystem. It brings the elegance and programmer-centric philosophy of Ruby to a modern, JIT-optimized runtime, enabling Ruby applications to benefit from advanced features like algebraic effects, multi-tier execution, and precise garbage collection.

## Features

- **Programmer Happiness**: Full support for Ruby's expressive syntax and dynamic object model.
- **Pure Object Orientation**: Maps Ruby's "everything is an object" model to Nyar's native object system and virtual dispatch.
- **Nyar Ecosystem Integration**:
  - **`nyar-jit`**: Optimizes hot Ruby methods and blocks into native machine code.
  - **`nyar-gc`**: Provides high-performance, precise memory management for managed Ruby objects.
  - **Algebraic Effects**: Used to implement Ruby's powerful control flow, including blocks, procs, and exception handling.
- **Dynamic Meta-Programming**: Supports Ruby's dynamic nature through Nyar's integrated metaprogramming primitives.

## Supported Constructs

- **Core Syntax**: `class`, `module`, `def`, `attr_accessor`.
- **Control Flow**: `if`, `unless`, `while`, `until`, `begin/rescue/ensure`.
- **Blocks & Procs**: Full support for Ruby's block syntax and closure model.
- **Standard Library**: Integrated with Nyar's FFI for core Ruby library support.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run application.rb
```

### Usage as a Library

```rust
use rusty_ruby::RustyRubyFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyRubyFrontend::new();
let ast = frontend.parse("def hello(name); puts \"Hello, #{name}!\"; end").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
