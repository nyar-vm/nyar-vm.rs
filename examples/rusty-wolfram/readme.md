# rusty-wolfram

Experimental Wolfram Language frontend for the Nyar VM.

## Overview

`rusty-wolfram` implements `nyar_types::NyarFrontend` for Wolfram/Mathematica-style syntax. Parsing uses `oak-wolfram` (`WolframBuilder`, `WolframRoot`); lowering walks the green tree and emits `chomsky` IR nodes through `NyarContext`. Coverage is driven by current tests and examples—not full Mathematica compatibility.

## Features

- **Oak-based parser** — incremental parse sessions via `oak_core::parser::ParseSession`.
- **Symbols and literals** — integers, floats, quoted strings, bare symbols.
- **Function application** — `Head[arg1, arg2, …]` lowered to `call`.
- **Binary operators** — arithmetic (`+`, `-`, `*`, `/`, `^`), comparisons, logic, rules (`->`, `:>`, `=`, `:=`), and Wolfram-specific forms (`/@`, `@@`, `//`, …).
- **Prefix / postfix** — unary `-`, `!`, postfix factorial, `&` pure functions (basic `#` lambda).
- **Lists** — `{ … }` lowered as extension nodes.
- **Module emission** — top-level statements become `rusty-wolfram-program` module body.

## Supported constructs (current)

| Syntax | Lowering |
|:---|:---|
| `Plus[1, 2]` | call to symbol `Plus` |
| `a + b` | `add_op` / dedicated binary mapping |
| `{1, 2, 3}` | `extension("list", …)` |
| `-x`, `x!` | `neg_op`, factorial extension |
| `f /@ g` | `map` |

## Usage

```rust
use nyar_types::NyarFrontend;
use rusty_wolfram::RustyWolframFrontend;

let frontend = RustyWolframFrontend::new();
let ast = frontend.parse("Plus[1, 2]").expect("parse");
// Pass `ast` to `lower_unified` inside a configured `NyarContext`.
```

## License

Licensed under MIT OR Apache-2.0.
