# backends

Target-specific backend implementations and minimal backend interfaces.

## Responsibilities

- Each backend explicitly declares its input type.
- Each backend implements `validate()` and `compile()` separately.
- `validate()` performs boundary checks only; it does not repair language semantics.
- `compile()` handles target encoding, layout, packaging, and artifact generation only.

## Constraints

- **CLR** backend accepts valid `ClrImage` or equivalent input only.
- **JVM** backend accepts `JvmClassFile`-style input only.
- **WASM** backend accepts `WasmModule`-style input only.
- **native** backend accepts native-lane low-level input only; it must not consume CLR/JVM/WASM container models directly.
- If input still has unclosed witness edges, non-static effect dispatch, or unresolved row/nominal facts, `validate()` must fail.

## Relationship to HIR / MIR / LIR

- Backends do not own HIR.
- Backends do not own language-level MIR.
- Backends consume only their route's LIR / backend input.
