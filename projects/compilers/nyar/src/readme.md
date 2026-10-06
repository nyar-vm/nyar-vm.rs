# nyar src

General optimization-backend orchestration source.

## Responsibilities

- Maintain neutral planning, target lanes, backend selection, and artifact orchestration protocol.
- Provide a shared orchestration layer for CLR / JVM / WASM / native / VM routes—not an extension of any one frontend.
- Offer language- and CLI-agnostic generic fixture / baseline validation utilities.

## Layering principles

- `src/abstractions` — minimal shared protocol only; no unified physical IR.
- `src/planning` — neutral analysis → `ArtifactPartitionPlan`.
- `src/packaging` — post-plan delivery protocol (`ArtifactSet`, `OutputSpec`, lane dispatch boundaries).
- `src/data_formats/*` — per-route containers and low-level inputs (`MSIL`, `PE`, `COFF`, …).
- Target-specific constraints belong in that route's data models, not in shared mega-types.

## Relationship to the language main chain

- Frontend-owned `AST / HIR / MIR / LIR` stay downstream.
- Downstream must close branded semantics into neutral analysis before entering `nyar`.
- `nyar` accepts only finished neutral planning input and forwards it to target encoding, layout, and packaging.

## Submodule roles

### abstractions
Minimal protocol: target family, backend input kind, artifact format, backend interface. No HIR, MIR, or unified LIR.

### planning
Build `ArtifactPartitionPlan` from neutral analysis. Express capabilities, runtime needs, lanes, and backend input boundaries. Do not re-parse frontend syntax.

### lanes
Lower each partition to the backend input its route consumes. No trait resolve, row closure, or effect handler selection.

### backends
Explicit input types; separate `validate()` / `compile()`. Honest rejection of bad routes and unclosed witness/effect edges.

### selection
Pick backends from requirements and priority. Selection only.

### data_formats
Low-level representation families after partitioning—not HIR, not language MIR, not cross-target LIR. `clr`, `msil`, `pe`, `coff` are real inputs for their routes.

### packaging
`ArtifactSet`, `OutputSpec`, sidecars, final delivery. No semantic reinterpretation.

### testing
Generic fixture collection, YAML sidecar IO, first-run generation, baseline diff. No `VALKYRIE` / `legion`-specific naming—generic fixture mechanics only.

## HIR / MIR / LIR vs this tree

- HIR / MIR definitions and transforms stay in upstream compilers.
- `nyar` does not define language HIR / MIR; it consumes target-specific results after `ArtifactPartitionPlan`.
- Internal `data_formats/*` are target LIR / backend inputs. Future `NyarIR` for CPU/VM is a CPU-route low-level form, not a shared final layer for all targets.

## Forbidden

- Do not close language semantics here.
- Do not rebuild a unified cross-target mega-IR.
- Do not let one frontend's facts hijack shared backend layers.
- Container layers must not patch upstream semantics at encode time.
