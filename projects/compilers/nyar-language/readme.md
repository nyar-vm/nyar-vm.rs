# nyar-language

Valkyrie language compiler and multi-host script frontends for the Nyar platform.

## Overview

`nyar-language` (package name) hosts the **Valkyrie main chain** (`AST → HIR → MIR SSA`) plus embedded host-language bridges (Bash, C, JavaScript, Lua, PowerShell, Python, Tcl, MSIL, PE, WAT, WIT, VON, AWSL). Valkyrie parsing is **Oak-only** in production (`oak-valkyrie`); this crate owns semantic lowering, not lexer/parser tables duplicated from Oak.

Public compile entry points include `ValkyrieCompiler::compile_source`, `compile_source_groups_to_artifacts`, and MIR helpers under `valkyrie::mir`.

## Main chain

| Stage | Location | Contract |
|:---|:---|:---|
| Parse | Oak (`oak-valkyrie`) via `valkyrie::frontend` | CST/AST only; no legacy text parser in production |
| HIR | `src/valkyrie/hir` | Resolve, type check, dispatch kinds (`static` / `witness` / `effect-handler`) |
| MIR (SSA) | `src/valkyrie/mir` | CFG, block params, explicit terminators; monomorphization and analysis |
| Plan / emit | `compile_pipeline`, re-exported `nyar::*` | `ArtifactPartitionPlan`, backend-neutral build output |

Legacy LIR exists only as an **internal** lane validation asset; it is not the public API surface.

## Host scripts

Modules under `src/bash`, `src/c`, `src/javascript`, etc. expose `evaluate_*` helpers and semantic bridges for REPL-style execution. Format trees for several of these also live in `std-data`; guest semantics stay here, not in the format layer.

## Features

- **LSP** under `valkyrie::lsp` (highlight, diagnostics, AWSL/VX handlers).
- **Formatter / text** re-exports for Valkyrie source tooling.
- **Derive** and **type_checker** pipelines for nominal types, effects, and rows closed in HIR.
- Re-exports **`nyar`** planning types (`ProgramFacts`, `ArtifactPartitionPlan`, `TargetProfile`, …) for product CLIs.

## Usage

```rust
use nyar_language::valkyrie::hir::ValkyrieCompiler;

let compiler = ValkyrieCompiler::default();
let hir = compiler.compile_source(source_text)?;
```

Full artifact builds go through `compile_source_groups_to_artifacts` with a configured backend bundle (see `legion` / product CLI).

## Boundaries

- No unified `god IR`; HIR and MIR stay semantically closed before partitioning.
- No runtime interpreter loop (see `nyar-vm` and `nyar-runner`).
- Do not re-implement Oak parser grammar inside this crate.
