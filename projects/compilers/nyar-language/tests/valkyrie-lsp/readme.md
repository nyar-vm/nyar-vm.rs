# Valkyrie LSP tests

Integration tests for the Valkyrie language server under `nyar-language::valkyrie::lsp`.

## Overview

These tests exercise LSP handlers against real compiler snapshots: diagnostics, completion, navigation, formatting hooks, and custom `valkyrie/*` methods where enabled.

Nyar itself is a compiler platform—not a monolithic LSP binary. Valkyrie hosts LSP on top of shared query infrastructure so other languages can reuse the same layering (`nyar-analyzer` contracts + language plugins).

## Running

```bash
cargo test -p nyar-language --test valkyrie-lsp
RUST_LOG=debug cargo test -p nyar-language --test valkyrie-lsp -- --nocapture
```

## Layout

| Area | Tests |
|:---|:---|
| Handler routing | Request → compiler query mapping |
| Diagnostics | Error conversion and publish filtering |
| Custom methods | AST/HIR inspection extensions when present |

Server launch (`--stdio` / TCP) and IDE wiring are product concerns; this directory focuses on handler correctness against fixtures.
