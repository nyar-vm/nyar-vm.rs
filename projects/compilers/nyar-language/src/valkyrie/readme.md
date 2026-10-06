# compiler src

Valkyrie compiler main-chain source.

## Responsibilities

- Maintain `HIR → Semantic MIR (SSA) → CanonicalProgram → RepresentationPlan`; target-private plans and encoding belong in `nyar-emitter`.
- Close semantics before targets so backends never patch language meaning.
- Supply the same frontend facts for the CLR mainline and JVM / WASM / native smoke guards.
- With `tests/spec`, enforce boundaries for `row`, `trait`, `class`, `sealed class`, `unite`, and `effect`.

## Forbidden

- No unified cross-target `god IR`.
- Do not grow lightweight planners into semantic buses.
- Target-agnostic layers must not hold target host implementation details.

## Single executable payload boundary

The frontend no longer exposes `mir_function_to_executable` or `mir_functions_to_executable_map`. Direct MIR→executable converters and standalone reachable-closure helpers were removed because they bypassed canonical validation and representation planning and could produce a second success payload for the emitter.

Value-storage tests that depended on old provider injection were removed with those entry points. That does **not** prove value storage, loops, runtime, or self-hosting are complete; coverage must be rebuilt from current sources through `CompiledProgram` and target-private preparation boundaries. Assembly summaries and internal callable name lookup still need convergence—deleting the old public API is not the same as finishing the single compile flow.
