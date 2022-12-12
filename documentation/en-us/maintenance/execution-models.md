# Nyar Execution Model

## 1. Overview

Nyar is designed to support multiple execution environments, ranging from high-performance production environments to highly interactive development environments.

## 2. Multi-Export Backend Architecture

Nyar employs a **Multi-Export Backend Architecture**. While the compiler frontend (AST -> HIR -> CFG -> SSA -> LIR) descends linearly, not all backends must start their transformation from LIR.

Each IR layer has its unique structural advantages, making it suitable for interfacing with different target platforms:

- **SSA (Static Single Assignment) -> LLVM IR**: 
    - LLVM itself is SSA-based. Converting directly from Nyar SSA preserves the richest data flow information, facilitating deep optimization by LLVM.
- **CFG (Control Flow Graph) -> WASM**: 
    - WASM is a structured control flow format. Converting directly from CFG makes it easier to recover the `block`, `loop`, `if`, and other structures required by WebAssembly, avoiding expensive control flow recovery after LIR flattening.
- **HIR (High-level IR) -> C / Source-to-Source**: 
    - HIR preserves high-level syntax information (e.g., Traits, type definitions). If exporting to C code or performing source-to-source translation is required, HIR is the best entry point.
- **LIR (Low-level IR) -> Interpretive Execution / AOT**:
    - LIR is designed for high-performance virtual machines, suitable for direct interpretive execution or simple AOT compilation into native machine code.

---

## 3. LIR Stack Model Decisions

Nyar's LIR chooses a **Stack Machine** model, based on the following core considerations:

### 3.1 Why Choose a Stack Model?

1.  **Simplicity of Code Generation**:
    - Stack-based bytecode is extremely easy to generate from an Abstract Syntax Tree (AST) or High-level IR (HIR).
    - It eliminates the need for complex register allocation algorithms (like linear scan or graph coloring) in the interpreter, speeding up the compilation pipeline.
2.  **Compact Instruction Format**:
    - Most stack instructions (e.g., `Add`, `Mul`, `Pop`) do not require operand fields, as they implicitly operate on the top of the stack.
    - This results in smaller bytecode files and reduced memory bandwidth during instruction fetching.
3.  **Cross-Platform Alignment**:
    - Many mainstream virtual machines (JVM, WASM, CLR) use stack-based architectures.
    - Choosing a stack model makes it more straightforward to lower Nyar's LIR to these target platforms without reinventing the execution logic.

### 3.2 Stack Frame Management

Each function call in Nyar creates a new `Frame` that contains:
- **Operand Stack**: A local stack for temporary values during expression evaluation.
- **Locals**: An array for storing local variables and parameters, accessed by index.
- **Upvalues**: References to variables in outer scopes for closure support.

This hybrid approach (Stack for calculation + Locals for storage) provides a balance between simplicity and performance.

### 4.1 Interpreted Execution

During development and debugging phases, Nyar uses its built-in virtual machine (`nyar-vm`) to directly interpret and execute LIR.

- **Advantages**:
  - **Rapid Startup**: No lengthy machine code generation process.
  - **Hot Reloading**: Allows replacing functions without recompiling the entire program.
  - **Deep Debugging**: The interpreter can directly access all runtime states.

### 4.2 Static Compilation

During the production deployment phase, LIR can be further lowered into target machine code (e.g., WASM or native instructions) via `nyar-aot`.

- **Target Platforms**:
  - **WebAssembly (WASM)**: Targeted at modern browsers and server-side WASM runtimes.
  - **Native Binary**: Generates high-performance machine code via backend generators (planned).

## 5. Runtime Features

Whether through interpretive execution or compiled execution, Nyar provides unified runtime support:

- **Algebraic Effects**: Achieves efficient control flow jumps through stack-based handlers.
- **Automatic Memory Management**: Memory management based on garbage collection (depending on configuration).
- **Metaprogramming Support**: Runtime reflection and dynamic type checking.
