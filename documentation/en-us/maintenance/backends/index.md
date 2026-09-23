# Backend Maintenance Guide

This document introduces the compilation flow and design considerations for various backends supported by the Nyar compiler.

## Overview

Nyar supports multiple backends, each with its own characteristics and optimization strategies. The general pipeline is:
`Source -> AST -> HIR -> CFG -> SSA -> LIR -> Target`

## Backend Implementations

- [JVM Backend](jvm.md): Targets the Java Virtual Machine, using a stack-based architecture.
- [WASM Backend](wasm.md): Targets WebAssembly, using a stack-based architecture.
- [CLR Backend](clr.md): Targets the .NET runtime, using a stack-based architecture.
- [Native Backend](native.md): Evaluation scheme for native instruction sets (x86, x64, arm64, riscv).
- [NyarVM](nyar-vm.md): Reference interpreter, using a stack-based architecture.

## Design Decisions

### LIR Phase for Stack Machines

As of 2026-01-30, Nyar VM uses a stack-based LIR.

**Rationale**:
- **Simplicity**: Mapping SSA to stack-based LIR is more direct than register allocation.
- **Performance**: Generating stack-based instructions allows for natural utilization of the operand stack and avoids excessive local variable access.
