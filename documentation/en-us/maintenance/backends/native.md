# Native Backend Evaluation and Proposals

This document evaluates the proposals for adding native instruction set (x86, x64, arm64, riscv) support to the Valkyrie compiler.

## Proposals Overview

There are three main paths to supporting native instruction sets, each with different entry points and workloads.

### Option A: Based on Cranelift (Recommended)

Cranelift is a high-performance, cross-platform code generator written entirely in Rust, intended as a lightweight alternative to LLVM.

- **Entry Point**: **SSA (Static Single Assignment)**
- **Advantages**:
    - Extremely fast compilation (suitable for JIT and fast AOT).
    - Pure Rust implementation, easy to integrate.
    - Already handles register allocation, instruction selection, and multi-platform support.
- **Workload**: Implement the conversion from `SsaProgram` to `Cranelift IR`.

### Option B: Based on LLVM

LLVM is the industry standard and provides the most powerful optimization capabilities.

- **Entry Point**: **SSA (Static Single Assignment)**
- **Advantages**:
    - Excellent runtime performance.
    - Supports almost all known hardware platforms.
    - Extremely mature ecosystem.
- **Disadvantages**:
    - Dependency on the heavy LLVM library (C++).
    - Long compilation times.
- **Workload**: Implement the conversion from `SsaProgram` to `LLVM IR`.

### Option C: Custom Backend (Register Machine Model)

Write an emitter directly on top of `LIR` for a specific architecture.

- **Entry Point**: **LIR (Low-level IR)**
- **Advantages**:
    - Zero external dependencies.
    - Full control over the generated machine code.
    - Can be deeply optimized for the Valkyrie runtime (e.g., special handling of algebraic effects).
- **Disadvantages**:
    - Enormous workload (requires writing a register allocator and instruction selector for each architecture).
- **Workload**: Implement an assembly emitter for x64/arm64 based on `LIR`.

## Technical Selection Recommendations

### 1. Short-term Goal: Cranelift
As the first native backend, **Cranelift** is the best choice. It matches Valkyrie's SSA structure perfectly and can satisfy both AOT and future JIT requirements.

### 2. Long-term Goal: LLVM
For release versions pursuing extreme performance, the LLVM backend can be introduced as an optional component.

### 3. Special Requirements: Custom LIR Transformation
If an extremely compact embedded runtime is required, or if existing backends have difficulty supporting Valkyrie's algebraic effects and continuations, consider generating machine code directly from LIR.

## Implementation Path

1. **Phase 1 (Research)**:
   - Evaluate whether `SsaProgram` in `valkyrie-types` contains all the semantic information required by Cranelift.
   - Experimentally implement a simple `Ssa -> Cranelift` converter supporting addition and return.

2. **Phase 2 (Basic Implementation)**:
   - Implement full type mapping.
   - Implement handling of calling conventions.
   - Implement linearization of basic blocks.

3. **Phase 3 (Advanced Features)**:
   - Handle algebraic effects (may require special stack operations or runtime support).
   - Implement debug information (DWARF) generation.
