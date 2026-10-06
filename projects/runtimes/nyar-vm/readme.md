# nyar-vm

Language-agnostic bytecode virtual machine for `.nyar` / Nyar IR modules.

## Overview

`nyar-vm` is the authoritative runtime that loads encoded Nyar modules, executes stack-based bytecode, coordinates optional JIT compilation, and exposes host import dispatch. Every guest language frontend lowers through an emitter into `.nyar` bytes before this crate sees the program. The VM must not import concrete language ASTs, parsers, or compiler pipelines.

Package name: `nyar-vm`. Rust import path: `nyar_vm`.

## Layering

| Component | Role |
|:---|:---|
| `NyarVm` | Public entry: `load`, `run`, `run_with_globals`, JIT/stack-map hooks |
| `Executor` | Interpreter loop, call frames, NJ1 compile cache, deoptimization handoff |
| `LoadedModule` / `ModuleGlobals` | Decoded module image and persistent global slots (singleton init) |
| `nyar-gc` (re-exported) | Managed heap, collector, layout descriptors, workload intent |
| `nyar-jit` (via `Executor::with_jit`) | Optional machine-code backend injected by the host |

## Features

- **Stack interpreter** with verified module loading and export resolution.
- **Module init functions** run once before entry calls when globals are first used.
- **Host imports** via `CallImport` and the `host` module (`HostOp` dispatch).
- **Algebraic effects** (`PerformEffect`), **async** (`Yield` / `Resume`), and **coroutine** objects on the managed heap.
- **NJ1 scalar fast path** with an in-executor compile cache (`invalidate_nj1_*` APIs).
- **JIT integration** through `JitCompiler`, stack maps, and deopt value encode/decode helpers.
- **Workload JSON** parsing for GC evidence and runtime tuning experiments.

## Usage

```rust
use nyar_bytecode::encode_module;
use nyar_vm::{NyarVm, Value};

let bytes = encode_module(&module_data);
let module = NyarVm::new().load(&bytes)?;
let mut vm = NyarVm::new();
let result = vm.run(&module, "main", vec![])?;
```

For tests and low-level control, construct `Executor` directly and call `run_function_frame`. Prefer `NyarVm` in product code.

## Boundaries

- Does **not** parse source code or lower HIR/MIR.
- Does **not** own the on-disk bytecode format contract (see `nyar-bytecode`).
- Re-exports GC types for convenience but heap policy lives in `nyar-gc`.
