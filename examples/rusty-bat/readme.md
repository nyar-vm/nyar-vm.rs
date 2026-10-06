# rusty-bat

Experimental Windows Batch (`.bat`) frontend for the Nyar VM.

## Overview

`rusty-bat` implements `nyar_types::NyarFrontend` on top of `oak-bat`. Parsing is functional; IR lowering remains a scaffold (commands and literals map to placeholder nodes).

## Current coverage

- `RustyBatFrontend::parse` → `oak_bat::ast::BatRoot`.
- Element lowering for commands, variables, strings, and text is not yet connected to real `chomsky` ops.

## Usage

```rust
use nyar_types::NyarFrontend;
use rusty_bat::RustyBatFrontend;

let frontend = RustyBatFrontend::new();
let ast = frontend.parse("@echo off\r\necho hello").expect("parse");
```

## License

Licensed under MIT OR Apache-2.0.
