# rusty-bash

Experimental Bash shell frontend for the Nyar VM.

## Overview

`rusty-bash` implements `nyar_types::NyarFrontend` using `oak-bash` for parsing. It is a **workspace example** for wiring Oak guest languages into `NyarContext`; lowering of commands and expansions is still largely stubbed.

## Current coverage

- Parse Bash source into `oak_bash::ast::BashRoot` via `RustyBashFrontend::parse`.
- `lower_unified` walks top-level elements but returns placeholder IDs for commands, variables, strings, and text nodes.

## Usage

```rust
use nyar_types::NyarFrontend;
use rusty_bash::RustyBashFrontend;

let frontend = RustyBashFrontend::new();
let ast = frontend.parse("echo hello").expect("parse");
```

## License

Licensed under MIT OR Apache-2.0.
