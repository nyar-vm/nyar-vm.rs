# abstractions

Target-neutral backend abstractions.

## Responsibilities

- Describe consensus types such as `TargetFamily`, `ArtifactFormat`, and `BackendInputKind`.
- Provide unified orchestration protocol per target lane, **not** a unified physical IR.

## Design constraints

- `BackendInputKind` declares what each backend actually consumes; inputs need not share one underlying structure.
- What may be unified here is lane selection, artifact description, and interface boundaries—not the physical representation of CLR / JVM / WASM / GPU.
- Adding a target means new route declarations and input kinds, not compatibility fields stuffed into old inputs.

## Interface boundary

- Future minimal protocol shape: `validate(TInput)` and `compile(TInput)`, not a new unified backend object.
- `validate()` rejects wrong routes, unclosed witness/effect edges, and inputs that violate target constraints.
- `compile()` consumes only validated input and performs target-specific lowering, encoding, layout, and packaging.
