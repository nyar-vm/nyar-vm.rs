# compiler tests

Valkyrie compiler integration tests organized by compile phase and semantic domain.

## Top-level modules

| Directory | Role |
|------|------|
| `smoke.rs` | End-to-end smoke: parse → HIR → MIR minimal path |
| `pipeline/` | `AST → HIR → MIR` main chain and scheduler |
| `control_flow/` | Control-flow validation, labels, try/?, fallthrough |
| `mir/` | MIR lowering, pattern dispatch, value layout |
| `type_checker/` | Type checking, constraints, patterns, overload |
| `typing/` | MRO (C3), inheritance conflict analysis |
| `oop/` | OOP witness, parent slots |
| `spec/` | Semantic specification tests |
| `optimizer/` | Static specialization, witness elimination |
| `derive/` | derive macro expansion |
| `module/` | Module graph and resolution errors |
| `highlight/` | Highlight text snapshot regression |

## Conventions

- Add new tests under the matching subdirectory; do not pile `.rs` files at `tests/valkyrie/` root.
- `spec/` may use `#[ignore]` for semantics not yet implemented.
- Regenerate highlight snapshots: `NYAR_TEST_REGENERATE=1 cargo test -p nyar-language text_fixture_highlight_regression`
