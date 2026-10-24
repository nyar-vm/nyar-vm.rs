# legacy-vm

Language-agnostic **PE / Futamura** interpreter substrate (GraalVM / Truffle-like).

**Not** “yet another language runtime.” This crate does **not** understand Lua / Bash / C grammar. It only provides: **guest registration**, **interpret dispatch**, and **specialize → native**.

Core job: frames / nodes / specialization / partial evaluation — **not** concrete language product logic.

## Design mental model

```text
                    ┌─ bash / lua / ps / tcl / c / …
                    │  (each owns its own AST + interpret)
                    ▼
              ┌─────────────┐
 legend ────► │ legacy-vm   │  guest seam + value bridge + dispatch
              │  PE substrate│
              └──────┬──────┘
                     │
          ┌──────────┼──────────┐
          ▼                     ▼
     run = interpret      specialize = PE (Futamura 1st)
     (daily fixtures)     → NativeResidual → x64 / Windows PE
```

| Concern | How |
| --- | --- |
| Weird / heterogeneous languages | **Plug-in guests**, not one shared AST. Each language keeps its own shape in `std-data` + `nyar-language`. |
| Daily execution | `LegacyVmRunner::run` → `GuestInterpretFn` → thin `evaluator/<lang>.rs` → `nyar-language` interpret |
| Compilation | Specialize the **interpreter** w.r.t. a program → **native-bound residual** → PE — **not** `nyar-vm` |
| Contrast with Valkyrie | Valkyrie folds languages into a shared IR then hand-emits many backends. legacy-vm unifies **registration / dispatch / specialize exit**, not syntax. |

## Ownership

| Layer | Owns |
| --- | --- |
| `algebra` / `compiler` / `value` / `guest` | Language-neutral PE substrate + guest registration seam |
| `runner` | Registration, detection, dispatch to guest evaluators |
| `evaluator/<lang>.rs` | **Thin** guest/runtime adapters only (`LegacyValue` ↔ guest values + delegate) |
| `nyar-language::src/<lang>/` | Concrete guest languages (`*Module` / `*SemanticBridge` / **interpret** / specialize) |

Analogy: Truffle = this crate; Truffle languages = `nyar-language` guests.

Guest languages plug in via a thin, language-neutral runtime ABI (`guest::GuestInterpretFn` / `GuestInterpret` + value bridge). Do **not** bake Lua / PowerShell / bash / … semantics into the PE core.

## How interpret works (any plugged-in language)

```text
source
  → std-data              parse into that language’s AST
  → nyar-language         that language’s interpret (semantic source of truth)
  → legacy-vm thin adapter  LegacyValue ↔ guest-local values, then delegate
  → guest seam            GuestInterpretFn registered on the Runner
  → legend                pick language, call run
```

The substrate only sees: given a language id, call a function pointer. Adding a new “weird” language means writing **parse + interpret** and registering a thin adapter — **not** changing the PE core. Architecture guards forbid owning a second full tree-walk inside `evaluator/`.

## How compile works (Futamura 1st projection)

```text
interpret subject ──specialize──► NativeResidualModule ──emit──► .exe (PE)
                                      │
                                      └── NOT nyar-vm / `.nyar` (product)
```

- Guest residualizes interpreter match arms into `nyar_language::ResidualSink`.
- Product sink: `compiler/native_residual.rs` (`NativeResidualSink` → `NativeResidualModule`).
- Windows PE wrap: `compiler/pe.rs` (`PeCompiler`).
- `StackResidualSink` / `.nyar` / `nyar-vm` are **probe-only** (e.g. JS/Python stubs), not the host-script PE destination.
- `legend run` stays on interpret; `legend build --target native` (aliases `pe` / `exe`) is the specialize path.

**Status:** Lua specialize → native residual is wired; full per-op x64 frame/stack codegen is still incomplete (integer exit-code fold / stub otherwise). Other host-script specialize paths may still be stubs.

## Forbidden

- New shared `HostScript*` / `host_script` abstraction (guests stay concrete in `nyar-language`).
- New full language tree-walks in this crate — add interpret in `nyar-language`, register a thin adapter here.
- Lexer/parser copies (parsers live in `std-data`).
- Treating `.nyar` / `nyar-vm` as the host-script PE product path.
- Equating this path with Valkyrie’s hand-written multi-backend (`CLR` / `JVM` / `WASM` / …) story — entering `.nyar` does **not** unlock that emitter chain.

## Tests / guards

```text
cargo test -p legacy-vm --test architecture_guards
cargo test -p legacy-vm --test golden_futamura
cargo test -p legacy-vm --test lang_<lang>
```

Layering checklist: [`../host-script-languages.md`](../host-script-languages.md).
