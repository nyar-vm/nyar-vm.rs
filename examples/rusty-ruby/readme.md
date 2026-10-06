# rusty-ruby

A Ruby language frontend for the Nyar VM.

## Overview

`rusty-ruby` brings Ruby's expressiveness and programmer-friendly philosophy to a modern JIT runtime, with algebraic effects, multi-tier execution, and precise GC.

## Features

- **Programmer experience**: Ruby expressiveness and dynamic object model.
- **Pure object-oriented**: "everything is an object" mapped to the Nyar object system and virtual dispatch.
- **Nyar ecosystem integration**:
  - **`nyar-jit`**: hot methods and blocks compiled to native code.
  - **`nyar-gc`**: high-performance precise collection for Ruby objects.
  - **Algebraic effects**: control flow for blocks, procs, and exceptions.
- **Dynamic metaprogramming**: Ruby dynamic features via Nyar metaprogramming primitives.

## Supported constructs

- **Core syntax**: `class`, `module`, `def`, `attr_accessor`.
- **Control flow**: `if`, `unless`, `while`, `until`, `begin/rescue/ensure`.
- **Blocks and procs**: Ruby block syntax and closure model.
- **Standard library**: core library support via Nyar FFI.

## Getting started

### Via Nyar CLI

```bash
nyar run application.rb
```

### As a library

```rust
use rusty_ruby::RustyRubyFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyRubyFrontend::new();
let ast = frontend.parse("def hello(name); puts \"Hello, #{name}!\"; end").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
