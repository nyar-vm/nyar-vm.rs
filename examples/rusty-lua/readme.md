# rusty-lua

A Lua language frontend for the Nyar VM.

## Overview

`rusty-lua` is a high-performance Lua frontend for the Nyar VM. It provides full lexical and syntactic analysis of Lua source code and translates it into Gaia IR. By targeting the Nyar VM, Lua scripts can benefit from advanced features like algebraic effects and multi-tier JIT compilation.

## Features

- **Lua 5.4 Compatibility**: Aims for high compatibility with the latest Lua specifications.
- **Fast Execution**: Translates Lua's register-based conceptual model into Nyar's stack-based execution, optimized by `nyar-jit`.
- **First-Class Closures**: Full support for Lua's powerful closure and upvalue system.
- **Table Support**: Efficient implementation of Lua tables using the VM's native object and dictionary support.
- **Coroutine Support**: Implements Lua coroutines using Nyar VM's native algebraic effects and delimited continuations.
- **Metatable System**: Integrated with the VM's virtual call and dynamic dispatch mechanisms.

## Supported Constructs

- **All standard Lua statements**: `if`, `while`, `repeat`, `for` (numeric and generic).
- **Functional Features**: Anonymous functions, multiple return values, proper tail calls.
- **Table Operations**: Literal construction, indexing, and iteration.
- **Environment Management**: Global (`_G`) and local variable management.

## Getting Started

### Usage via Nyar CLI

```bash
nyar run script.lua
```

### Usage as a Library

```rust
use rusty_lua::RustyLuaFrontend;
use nyar_types::NyarFrontend;

let frontend = RustyLuaFrontend::new();
let ast = frontend.parse("print('Hello from Lua on Nyar!')").unwrap();
// Lower and execute via NyarVM
```

## License

Licensed under MIT OR Apache-2.0.
