# nyar-jit

Optional just-in-time compiler for Nyar bytecode.

## Overview

`nyar-jit` accelerates hot functions by compiling verified bytecode views into machine code while preserving deoptimization back to the interpreter. It consumes **only** bytecode-level facts (function specs, stack maps, scalar programs) and must not depend on `nyar-language`, `nyar-vm`, or guest language crates—avoiding circular dependencies. The VM injects a `JitCompiler` implementation via `Executor::with_jit`.

## Features

- **`JitCompiler` trait** — `DisabledJit` (default no-op) and `StackMapJit` for stack-map-aware compilation.
- **`BaselineScalarJit`** — matches and compiles small scalar programs (`ScalarProgram`, NJ1 blob encoding).
- **`JitCompileRequest` / `JitFunctionSpec`** — describe what to compile.
- **`JitCompiledArtifact` / machine-code encoders** — ret/void, local/binop/cmp/select peephole sequences with `MACHINE_CODE_MAGIC`.
- **`DeoptMap` / `RestoredInterpreterFrame`** — rebuild interpreter frames when assumptions break.
- **`FunctionStackMaps` / `StackMapEntry`** — shared contract with `nyar-vm` for safe deopt.
- **`JitAssumption` / `baseline_scalar_assumptions`** — document assumptions checked at runtime.

## Usage

```rust
use nyar_jit::BaselineScalarJit;
use nyar_vm::Executor;

let mut ex = Executor::with_jit(Box::new(BaselineScalarJit::new()));
```

Product code typically configures JIT through `NyarVm` / executor setup rather than calling machine-code encoders directly.

## Boundaries

- Does not replace `nyar-aot` offline artifacts; online acceleration only.
- Does not load modules or run the full interpreter loop.
- Scalar encoders are building blocks for tests and baseline JIT, not a general LLVM backend.
