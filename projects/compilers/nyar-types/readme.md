# nyar-types

Shared foundational types for the Nyar platform.

## Overview

`nyar-types` holds cross-frontend, cross-analyzer, and cross-backend contracts that must stay stable without importing language compilers or runtimes. It is intentionally **not** a god-object IR crate.

## Major modules

| Module | Purpose |
|:---|:---|
| `semantic_ids` | Parameterized MIR identities (`TypeId`, `ItemId`, `OperatorId`, …) |
| `canonical_program` | Canonical MIR/program pipeline types and staged compile results |
| `executable` | **Backend-private** lowering view (`Instruction`, `Terminator`, suspend plans) |
| `layout` | Aggregate / singleton / sum-type layout plans for codegen |
| `neutral_contract` | Auditable artifact contracts, evidence, provenance |
| `external_import` | Stable host import/call edge descriptions |
| `registries` | Extensible attribute and operator registration |
| `ty` / `symbols` / `source` | Core type, naming, and source-span primitives |
| `witness_submission` | Witness call edges and method slot submissions |
| `contract_versions` | Identity / MIR / layout schema version fingerprints |

## `CapabilityTag`

Lightweight string tags attached to `ProgramFacts` and backend requirements so planners can gate lanes (`gpu.shader`, reference management, etc.).

## Usage

```rust
use nyar_types::{CapabilityTag, ExternalImportLink, QualifiedName};

let cap = CapabilityTag::new("reference.gc");
```

## Boundaries

- Does **not** define language-level HIR / MIR / LIR branded to Valkyrie.
- Does **not** embed PE/COFF/MSIL containers or VM interpreter state.
- `executable` is a lowering view, not a public semantic bus across languages.
