# nyar-analyzer

Frontend-neutral analysis contracts for the Nyar platform.

## Overview

`nyar-analyzer` consumes **already-closed** facts from language frontends and expresses them in a target-agnostic shape for `nyar` planning, lane selection, and packaging. It does not own Valkyrie `HIR` / `MIR` / `LIR` and does not parse source text.

## Core types

| Type | Role |
|:---|:---|
| `ProgramFacts` | Module-level summary: entries, imports/exports, functions, nominal types, capabilities |
| `EntryContract` / `ImportContract` / `ExportContract` | Neutral linking conventions |
| `FunctionAnalysis` | Suspend/async/host-interop flags, external import links, reference-management hints |
| `TypeDefinitionFact` | Enum / unite / flags layout facts for downstream backends |
| `RuntimeRequirement` | Declared runtime needs (re-exported from `nyar-types`) |

Helper methods on `ProgramFacts` include `primary_entry`, `requires_capability`, and reference-management aggregation across functions.

## Subsystems

- **`highlight`** — platform contract for syntax highlighting (implementations live in frontends).
- **`format`** — source formatter + printer + `Document` engine contract.
- **`report`** — SSG / hydrate island specs (`ColSeriesItem`, `HydratedChartSpec`, …) for AWSL tooling.

## Usage

```rust
use nyar_analyzer::ProgramFacts;

if facts.requires_capability("gpu.shader") {
    // route to GPU lane in nyar planning
}
```

Frontends populate `ProgramFacts` after semantic closure; `nyar` reads it to build `ArtifactPartitionPlan`.

## Boundaries

- No dependency on concrete guest-language parser crates.
- No backend container encoding or tarball packaging.
- No language-specific highlighter / formatter implementations in this crate.
