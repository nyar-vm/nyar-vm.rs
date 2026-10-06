# rusty-cmd

Experimental Windows `cmd.exe` script frontend for the Nyar VM.

## Overview

`rusty-cmd` wires `oak-cmd` into the Nyar frontend trait surface. Like `rusty-bash` / `rusty-bat`, it proves Oak → `NyarFrontend` integration while semantic lowering is still incomplete.

## Current coverage

- Parse into `oak_cmd::ast::CmdRoot`.
- `lower_element` returns placeholders for commands and literal-like elements.

## Usage

```rust
use nyar_types::NyarFrontend;
use rusty_cmd::RustyCmdFrontend;

let frontend = RustyCmdFrontend::new();
let ast = frontend.parse("echo hello").expect("parse");
```

## License

Licensed under MIT OR Apache-2.0.
