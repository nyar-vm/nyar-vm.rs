# nyar-emitter

Bundled backend driver for partitioned compiler output.

## Overview

`nyar-emitter` consumes completed `PartitionBackendRequirement` values and target-specific inputs, routes them to bundled family compilers, and returns `ArtifactSet` entries plus entry/run contracts for orchestrators such as `legion`.

## Layering

| Area | Role |
|:---|:---|
| `src/lib.rs` | Stable public API, shared request/response models |
| `src/driver/*` | Backend routing, partition orchestration, family registration |
| `src/driver/families/*` | One file per backend family (CLR, JVM, Wasm, native, nyar-vm, …) |
| `src/backend/*` | Target backend bodies without driver selection logic |
| `src/artifacts/*` | Sidecars and packaging helpers |
| `src/lowering/*` | Fragment → driver input lowering |

Family compilers register as `BackendCandidate` instances; `nyar::BackendSelector` performs unified selection (driver does not duplicate selection algorithms).

## Family compilers

- `driver/families/clr.rs` — CLR bundled chain
- `driver/families/jvm.rs` — JVM artifacts and run contract
- `driver/families/wasm.rs` — WASM / WASI artifacts
- `driver/families/native.rs` — native object files
- `driver/families/nyar_vm.rs` — `.nyar` sidecars and VM modules
- `driver/partitioning.rs` — partition → family mapping and report merge
- `artifacts/suspend_sidecar.rs` — suspend sidecar serialization

## Single compile-flow contract (in progress)

Production still carries transitional duplication: `BackendPrivatePlan` is generated from `CompiledProgram`, but outer layers may still supply `AssembledFragment` / `FragmentSubmission` summaries consumed by backends. Treating the private plan type as wired **does not** mean the single-owner contract is finished.

- Do not merge `ArtifactPartitionPlan` with `RepresentationPlan` by name alone; remove fields that duplicate canonical facts used as semantic authority during lowering.
- Wasm lowering lives under `lowering/backends/wasm/mir/`; deleted unreachable copies are cleanup only, not proof of zero side paths.
- End-to-end validation must drive the compiler from source; hand-filled legacy fragments are not acceptance evidence.

## Usage

Product CLIs call `compile_with_bundled_backends` (and related driver APIs) after `nyar-language` produces partitioned plans. See `testing` module for integration-test wrappers.

## Boundaries

- Does not parse Valkyrie source.
- Does not run artifacts (see `nyar-runner` / `nyar-vm`).
