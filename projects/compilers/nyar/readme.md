# nyar

Multi-frontend optimization backend orchestration for the Nyar platform.

## Overview

`nyar` consumes neutral **`ProgramFacts`** from `nyar-analyzer` and object-algebraic input from `nyar-optimizer`, then produces **`ArtifactPartitionPlan`**, target lanes, backend selection, and packaging protocols. It unifies **orchestration**, not a single physical IR shared by every target.

## Pipeline

```text
ProgramFacts
  -> Object Algebraic Program
  -> E-Graph Optimization Session
  -> Futamura Projection Plan
  -> ArtifactPartitionPlan
  -> Target Lowering Lane
  -> Target-specific LIR / Backend Input
  -> validate()
  -> compile()
  -> OutputSpec
  -> ArtifactSet
```

What is unified is how partitions reach the correct route and deliver artifacts—not one backend IR for CLR, JVM, GPU, and VM alike.

## Upstream boundary

Language semantics (`row`, `trait` / `imply`, `class` / `sealed class`, `unite`, `effect`, …) must close **before** `nyar`:

- Row method requirements become closed member facts in HIR, not downstream row witnesses.
- Open dispatch stays explicit (`witness`, `effect-handler`); never disguised as static calls.
- Nominal subtyping and `unite` exhaustiveness are upstream facts; backends must not re-infer them.
- Routes that cannot support open witness or effect dispatch must fail before emit, not at encode time.

## HIR / MIR / LIR roles

### HIR

- First semantic closure: resolve, type check, bind constraints, desugar surface syntax.
- Calls are tagged: static, witness, or effect-handler dispatch.
- Rows collapse to selected members; traits retain witness/evidence; effects retain handler bindings.
- No monomorphization, partitioning, or target layout decisions here.

### MIR (SSA)

- Primary analysis IR: CFG, block parameters, explicit terminators and value deps.
- Monomorphization, escape analysis, effect summaries, closed-world expansion.
- Witness and effect operands remain explicit for analysis passes.

### LIR

- Not one shared type—a **family** after partitioning:
  - CPU/VM → Nyar-style low-level IR
  - CLR → ECMA-335-oriented (`ClrImage`, MSIL, PE)
  - JVM → classfile model
  - WASM → structured control flow + sections
  - GPU → DXIL / SPIR-V / MSL directly

`src/data_formats/*` holds target containers and low-level encodings, not a language-level LIR superset.

## Crate layout

| Path | Role |
|:---|:---|
| `src/abstractions` | `TargetFamily`, `BackendInputKind`, minimal backend traits |
| `src/planning` | Facts → `ArtifactPartitionPlan` |
| `src/lanes` | Partition → target-specific input |
| `src/selection` | Backend choice per lane/input |
| `src/backends` | Per-backend `validate()` / `compile()` |
| `src/data_formats/*` | MSIL, PE, COFF, ClrImage, … |
| `src/packaging` | `OutputSpec`, `ArtifactSet`, sidecars |

## GPU shader path

- Unified protocol, not unified GPU physical IR.
- Flow: `ShaderDecl → OA/ENode → E-graph → ArtifactPartitionPlan (Gpu) → SPIR-V/DXIL emit`.
- NyarVM is a separate route—not a GPU compile hop.
- Extend via fragment rewrite theories (`graphic` / `neural`) and lane-specific emitters; no unified GPU IR fallback.

CI gates: `cargo test -p nyar --test gpu_fragment`, `cargo test -p legion artifact_formats`.

## Boundaries

- No single-language AST/HIR/MIR/LIR owned here.
- No cross-language god IR.
- Container models must not patch language semantics at encode time.
