# NyarVM Maintenance Guide

NyarVM is the internal reference interpreter for Nyar.

## Compilation Pipeline

`Source -> AST -> HIR -> CFG -> SSA -> LIR -> VM Execution`

### 1. AST -> HIR
- **Responsible for**: Desugaring, scope resolution, and handling shadowing.
- **Implementation**: Provided by the frontend (e.g., `oak-rust`).

### 2. HIR -> CFG
- **Responsible for**: Converting structured control flow (`if`, `while`, `loop`) into basic blocks and jumps.
- **Implementation**: `nyar-aot`.

### 3. CFG -> SSA
- **Responsible for**: Constructing Static Single Assignment form and inserting Phi nodes.
- **Implementation**: `nyar-aot`.

### 4. SSA -> LIR
- **Responsible for**: Phi elimination and stack-based instruction lowering.
- **Implementation**: [compiler.rs](file:///e:/%E6%99%AE%E9%81%8D%E4%BC%98%E5%8C%96/nyar-vm/projects/nyar-vm/src/bytecode/compiler.rs).
- **Current Status**:
    - [x] Basic Phi elimination (via Move insertion at the end of predecessor blocks).
    - [x] Basic instruction lowering to stack-based bytecode.
    - [ ] Optimized stack usage (minimizing Push/Pop sequences).
    - [x] Local variable slot allocation.
    - [x] Stack depth calculation for VM frames.

### 5. VM Execution
- **Responsible for**: Executing LIR instructions using a stack-based interpreter, managing the call stack, heap, and effect handlers.
- **Implementation**: [interpreter.rs](file:///e:/%E6%99%AE%E9%81%8D%E4%BC%98%E5%8C%96/nyar-vm/projects/nyar-vm/src/vm/interpreter.rs).
- **Current Status**:
    - [x] Basic arithmetic and logic instructions.
    - [x] Jumps and branches (Jmp, JmpIf).
    - [x] Function calls and returns (supporting Native and FFI).
    - [x] Object and array operations (Alloc, Load, Store).
    - [x] Structured error handling (Effect, Continuation).
    - [x] Code coverage statistics.

### 6. FFI Mechanism
- **Design**: FFI markers are attached to `micro` function declarations via annotations (e.g., `@import`).
- **Advantages**:
    - **Type Safety**: Using the function's `parameters` and `returns` definitions, the compiler can accurately generate marshaling code.
    - **Multi-backend Adaptation**: Interpreted and executed by different backends via the `target` parameter (e.g., `wasm`, `jvm`, `clr`, `dll`).
- **Example**:
    ```nyar
    @import(target: wasm, "wasi:random/insecure", "get-insecure-random-u64")
    micro get_random_u64() -> u64
    ```

### 7. Deep JIT Optimization
- **Trigger Level**: The JIT compiler works at the **SSA / HIR** level, not LIR.
- **Core Logic**:
    - **Semantic Retention**: Retains full type information, object boundaries, and explicit effect flows at the SSA level.
    - **Optimization Capabilities**:
        - **Cross-function Inlining**: Deep inlining based on hotspots at the SSA level.
        - **Type Specialization**: Generating type-specific fast paths at the SSA level using runtime feedback.
        - **Memory Optimization**: Performing more accurate escape analysis to scalarize objects and allocate them to registers.

### Stack Machine
The LIR of NyarVM is a stack-based instruction set, optimized for simplicity and easy to generate from HIR/SSA.

### Phi Elimination
SSA is lowered to LIR by eliminating Phi nodes and mapping SSA variables to stack slots or local variable indices.
