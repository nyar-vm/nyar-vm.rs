# nyar-optimizer

Object-algebraic optimization skeleton for the Nyar platform.

## Overview

`nyar-optimizer` provides **E-Graph** sessions, rewrite rules, and **Futamura projection** families over object-algebraic programs. It sits between closed semantic input and `nyar` artifact planning—not a single unified IR enum.

## Responsibilities

- Accept object-algebraic program boundaries after upstream semantic closure.
- Run equivalence rewriting and extraction strategies inside an E-Graph session.
- Select `futa_*` projection families per target family instead of emitting one closed IR shape.
- Model host boundaries separately from reference-object management (JS glue / WASI components may share GC reference semantics but not host binding rules).

## Current status

- Combination interfaces, rule theories, and projection edges are fixed first.
- Object algebra is **not** collapsed into a single node enum.
- Futamura projection is a target-family transform, not a pre-emit alias step.

## Boundaries

- Does not duplicate `nyar-analyzer::ProgramFacts` structures.
- Does not merge all backends into one god IR.
- Target-specific encoders (MSIL, SPIR-V, …) live in `nyar` / `nyar-emitter`, not here.
