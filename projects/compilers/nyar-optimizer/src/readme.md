# nyar-optimizer (src)

Object-algebraic optimization skeleton for the Nyar platform (rustdoc source).

## Responsibilities

- Accept object-algebraic program boundaries after upstream semantic closure.
- Maintain equivalence rewrite rules, E-Graph sessions, and extraction strategies.
- Select `futa_*` projection families per target family instead of emitting one closed unified IR.

## Current boundaries

- Combination interfaces, rule theories, and projection edges are fixed first.
- Object algebra is **not** collapsed into a single node enum.
- Futamura projection is a target-family transform, not a pre-emit alias step.

## Forbidden

- Do not duplicate `nyar-analyzer::ProgramFacts` structures.
- Do not merge all backends into one god IR.
- Do not mislabel target-specific encoders as a unified backend representation.
