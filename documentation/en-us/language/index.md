# Valkyrie Compiler Intermediate Representation Architecture

The Valkyrie compiler employs a multi-layered Intermediate Representation (IR) architecture, providing a highly efficient compilation and optimization infrastructure for the language. Through four stages of progressive transformation, Valkyrie translates high-level language features into optimized target code, supporting various execution environments such as WebAssembly, JavaScript, and native code.

## Compiler Architecture Overview

### Design Goals

The design of the Valkyrie compiler focuses on:
- **Expressive Transformation**: Safely lowering complex functional and algebraic effect features.
- **Multi-level Optimization**: Providing optimizations across multiple levels, from high-level semantics to low-level instructions.
- **Cross-platform Support**: A unified backend architecture supporting multiple target platforms.
- **Development Toolchain**: Providing precise metadata support for IDEs and debuggers.

### Intermediate Representation Transformation Flow

The compilation process of Valkyrie is modeled as a progressive lowering process from source code to machine code:

#### 1. Frontend (Frontend - Oaks)
- **Input**: Source code
- **Output**: AST (Abstract Syntax Tree) -> HIR (High-level IR)
- **Responsibilities**: Syntax parsing, name resolution, type checking, Trait specialization, pattern matching desugaring.
- **Implementation**: [oak-valkyrie](file:///e:/普遍优化/oaks/examples/oak-valkyrie)

#### 2. Lowering (Lowering - Chomsky UIR)
- **Input**: HIR
- **Output**: UIR (Universal Intermediate Representation / IKun)
- **Responsibilities**: Converting the high-level semantic graph into a universal intermediate representation based on E-Graphs, preparing for global optimization.
- **Implementation**: [valkyrie-compiler](file:///e:/普遍优化/valkyrie.rs/projects/valkyrie-compiler)

#### 3. Optimizer (Optimizer - Nyar VM / Chomsky)
- **Input**: UIR
- **Output**: Optimized UIR (IKun Tree)
- **Responsibilities**: Whether in AOT or JIT mode, all core optimization tasks (constant folding, dead code elimination, loop optimization, etc.) are driven by the **Chomsky optimization engine** powered by **Nyar VM**.
- **Features**: Based on E-Graph Equality Saturation technology, capable of discovering deep optimization spaces that are difficult for traditional compilers to reach.
- **Implementation**: [ProjectChomsky](file:///e:/普遍优化/ProjectChomsky)

#### 4. Backend (Backend - Nyar VM / Gaia)
- **Input**: Optimized UIR
- **Output**: Target machine code (AOT) or memory-executable code (JIT)
- **Responsibilities**: Register allocation, instruction selection, stack frame management, and final code emission.
- **Implementation**: [nyar-vm](file:///e:/普遍优化/nyar-vm) and [project-gaia](file:///e:/普遍优化/project-gaia)

## Responsibilities of Each IR Layer

### [AST - Abstract Syntax Tree](./ast/index.md)

The AST layer serves as the entry point for the compiler, receiving the syntax tree from the parser and providing a unified abstraction of language features:

- **Language Feature Mapping**: Mapping the syntactic structure of source code to structured AST nodes.
- **Error Recovery**: Providing robust syntax error reporting and recovery mechanisms.
- **Tooling Support**: Providing precise source code location information for LSP.

**Core Supported Features**:
- Functional Programming: Function definitions, closures, higher-order functions.
- Object-Oriented: Classes, interfaces, inheritance, polymorphism.
- Generic Programming: Type parameters, constraints, specialization.
- Pattern Matching: Enums, destructuring, guard conditions.
- Algebraic Effects: Effect declarations, handlers, resumption.

### [HIR - High-level Intermediate Representation](./hir/index.md)

The HIR layer is the core of semantic analysis, responsible for converting the AST into an intermediate representation with complete type and semantic information:

- **Type System**: Performing complex type inference and static checking.
- **Semantic Analysis**: Handling name resolution, scope analysis, and borrow checking.
- **Semantic Lowering**: Translating high-level syntactic sugar into more fundamental semantic forms.

**Architectural Advantages**:
- Strict validation of the type system.
- Precise diagnosis of semantic errors.
- Support for intelligent code completion (IntelliSense).

### [MIR - Mid-level Intermediate Representation](./mir/index.md)

The MIR layer is the hub for optimization, converting high-level semantics into Control Flow Graphs (CFG) for deep program analysis:

- **Control Flow Construction**: Converting complex control structures (e.g., effect handlers, loops) into basic block graphs.
- **Data Flow Analysis**: Performing optimizations like constant propagation and dead code elimination based on SSA form.
- **Ownership Analysis**: Handling resource lifecycles and move semantics at a mid-level stage.

### [LIR - Low-level Intermediate Representation](./lir/index.md)

The LIR layer is the core of code generation, providing a unified low-level abstraction for multiple target platforms:

- **Instruction Selection**: Optimizing selection for virtual instruction sets.
- **Multi-backend Support**: Uniformly generating WebAssembly, JavaScript, or native machine code.
- **Platform-Specific Optimization**: Fine-tuning for different execution environments (e.g., browsers, WASI).

## Core Value of the Compiler

Through this multi-layered architecture, Valkyrie ensures that code maintains high-level abstractions while achieving near-native execution performance. Developers can leverage cutting-edge features like algebraic effects without worrying about runtime performance overhead, as the compiler optimizes them into efficient control flows through deep static analysis during the transformation process.
