# compiler tests spec

Semantic specification tests.

## Goals

- Encode executable guardrails for `row`, `trait`, `class`, `sealed class`, `unite`, and `effect` boundaries first.
- Establish scenarios, naming, and expected outcomes even when implementation is incomplete.
- Prevent `HIR`, `MIR`, `nyar`, or any backend route from silently changing semantics.

## Groups

- `row.rs` — anonymous `trait` row methods only; no associated types.
- `trait_system.rs` — named trait satisfaction must land on named witness.
- `nominal.rs` — `class` / `sealed class` use nominal subtyping only; `unite` accepts only declared variant sets.
- `overload.rs` — fixed overload priority and ambiguity rules.
- `associated_types.rs` — associated types belong to named traits only and must resolve uniquely.
- `diagnostics.rs` — errors must distinguish nominal, row, trait, and effect failures.
- `backend_boundary.rs` — backends must not re-run row/nominal decisions.

## Strategy

- Prefer spec tests before implementation tests.
- Use `#[ignore = "..."]` for scenarios not yet implemented.
- Do not leave critical semantics as oral convention because tests fail today.
