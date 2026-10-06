# rusty-lua

A Lua language frontend for the Nyar VM.

## Overview

`rusty-lua` provides full lexical and syntactic analysis, compiling Lua source into Gaia IR. Lua scripts benefit from algebraic effects and multi-tier JIT.

## Features

- **Lua 5.4 compatibility**: high-compatibility target aligned with the latest Lua specification.
- **Fast execution**: maps the Lua register conceptual model to Nyar stack execution, optimized via `nyar-jit`.
- **First-class closures**: full closure and upvalue support.
- **Table support**: Lua tables via VM objects and dictionaries.
- **Coroutines**: Lua coroutines via algebraic effects and delimited continuations.
- **Metatables**: integrated with VM virtual dispatch and dynamic dispatch.

## Supported constructs

- **Standard statements**: `if`, `while`, `repeat`, `for` (numeric and generic).
- **Functional**: anonymous functions, multiple return values, proper tail calls.
- **Table operations**: literal construction, indexing, and iteration.
- **Environment**: global `_G` and local variable management.

## Getting started

### Via Nyar CLI

```bash
nyar run script.lua
```

### As a library

```rust
use rusty_lua::RustyLuaFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyLuaFrontend::new();
let ast = frontend.parse("print('Hello from Lua on Nyar!')").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
