# nyar-runner

Host runtime dispatch for `legion run` and native/Wasm/Node embedding.

## Overview

`nyar-runner` schedules **compiled artifacts** to target runtimes. It owns FFI, effect hooks, runner registration, and target descriptors—not the Valkyrie language main chain.

## Target modules

| Module | Runs |
|:---|:---|
| `src/jvm` | Java / JVM artifacts |
| `src/clr` | .NET / CLR |
| `src/wasm` | Node + Wasm JS glue |
| `src/wasi` | Wasmtime / WASI |
| `src/windows` | Native `.exe` |

`nyar-wasm` and `nyar-napi` re-export this crate as the shared host surface for platform packages.

## Responsibilities

- Runtime registry and runner selection (`RunnerFamily`, `RunnerSelector` from `nyar`).
- Load compiler output and invoke the correct OS/process/embedder.
- Surface host interop required by suspended functions and imports.

## Boundaries

- No HIR / MIR / type checking.
- No bypass of CLR bootstrap requirements via ad-hoc host bridges.
- Compilation stays in `nyar-language` + `nyar-emitter`; this crate executes results.
