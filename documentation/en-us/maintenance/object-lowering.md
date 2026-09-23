# Valkyrie Compiler: Lowering Guide

## 1. Lowering Philosophy: Progressive Refinement

The core architecture of the Valkyrie compiler is built upon **Progressive Lowering**. Compilation is not a one-time transformation but a journey from high abstraction to extreme specificity.

```mermaid
graph TD
    subgraph Frontend
        A[Source Code] -->|Parse| B(<b>oak-valkyrie</b><br><i>Syntax Structure</i>);
    end

    subgraph Mid-end (The Lowering Pipeline)
        B -->|<b>HIR Lowering</b>| C(<b>HIR</b><br><i>Types, Scopes, Traits</i>);
        C -->|<b>CFG Lowering</b>| D(<b>CFG</b><br><i>Basic Blocks, Control Flow</i>);
        D -->|<b>SSA Transform</b>| E(<b>SSA</b><br><i>Data Flow Optimization</i>);
        E -->|<b>LIR Lowering</b>| F(<b>LIR</b><br><i>Virtual Instructions, Registers</i>);
    end

    subgraph Backends (Multi-Export)
        C -.->|Source Export| G[<b>C Backend</b>];
        D -.->|Structural Export| H[<b>WASM Backend</b>];
        E -.->|Flow Export| I[<b>LLVM Backend</b>];
        F -->|Execution| J[<b>Valkyrie VM</b>];
    end
```

## 2. Multi-level Backend Lowering

To balance execution efficiency and multi-platform compatibility, Valkyrie allows backends to interface at different IR levels.

### 2.1 Transformation Advantages of Each IR Layer

| Interface Level | Target | Core Advantage |
| :--- | :--- | :--- |
| **HIR** | C / TypeScript | Preserves full high-level syntax and semantics, suitable for source-level export and cross-language interoperability. |
| **CFG** | WebAssembly | WASM requires structured control flow. At the CFG level, the Relooper algorithm can more easily generate `loop` and `if` structures. |
| **SSA** | LLVM IR | LLVM is an SSA-based optimizer. Interfacing at this level allows seamless integration with LLVM's global optimizations (GVN, SCCP, etc.). |
| **LIR** | VM / Native | A linear instruction set optimized for the Valkyrie runtime, suitable for interpretive execution, JIT, or generating highly customized AOT machine code. |

### 2.2 Difficulties in Converting LIR to WASM/LLVM

While it is possible to convert from LIR to WASM or LLVM, significant difficulties exist:

1.  **Loss of Control Flow**: LIR is a flat instruction stream based on jumps and branches. WASM requires strict structural nesting, making the recovery of control flow structures from LIR extremely expensive and complex.
2.  **Registers vs SSA**: LIR uses physical-style virtual registers. Converting to LLVM SSA requires re-performing "register promotion" (Mem2Reg), which essentially reverses the LIR generation process.
3.  **Low-level Implementation of Algebraic Effects**: `Yield/Raise` in LIR is implemented based on runtime Frame snapshots. WASM or LLVM does not directly support such fine-grained state preservation, requiring implementation through special transformations (e.g., Asyncify or Continuation Passing Style), which results in significant performance loss.

## 3. Feature Lowering Example: Pattern Matching

Pattern matching is a core feature of Valkyrie. We will trace how it lowers from AST to final control flow.

### 3.1 AST Phase
In AST, `match` is a node that directly reflects the syntax:
- `ast::Match { scrutinee, arms }`
- Each arm contains a `pattern`, `guard`, and `body`.

### 3.2 HIR Phase: Pattern Desugaring and Decision Trees
In HIR, the `match` expression undergoes key transformations:
- **Pattern Desugaring**: 
  - Decomposes complex nested patterns (e.g., `Some(Point { x: 1, .. })`) into simple primitive tests:
    1. Test if it is `Some`.
    2. Bind the internal value to a temporary variable.
    3. Test if the `x` field of that value equals `1`.
- **Decision Tree Construction**: 
  - The compiler builds a logical decision tree, optimizing the matching order to reduce redundant tests.
  - **Exhaustiveness Check**: Uses a space-covering algorithm to ensure all possible inputs are covered, otherwise generating a compilation error.

### 3.3 CFG Phase: Making Control Flow Explicit
In CFG, the decision tree is transformed into specific basic block structures:
- **Branch Expansion**: Each decision node is transformed into a `SwitchInt` or `If` terminator.
- **Binding Promotion**: Variables bound in pattern matching (e.g., `x` in `let Some(x) = ...`) are assigned values upon entering the corresponding branch block.
- **Error Block**: For uncovered cases (if the compiler allows non-exhaustive matching), it jumps to an explicit `Panic` block.

## 4. Feature Lowering Example: Algebraic Effects

Valkyrie's Algebraic Effects are implemented through **Continuations** and a **Handler Stack**.

### 4.1 HIR Phase
- **Effect Type Checking**: Ensures that all effects triggered within a `try` block are defined in a `handle` block or declared in the function signature.
- **Closure Capture**: Transforms each branch in a `handle` into an implicit closure that receives effect parameters and a resumption point (resume).

### 4.2 CFG Phase: Explicit Stack Operations
Algebraic effects introduce special control flow instructions at the CFG level:
- **`PushHandler`**: Before entering a `try` block, pushes a reference to the current handler onto the VM's handler stack.
- **`PopHandler`**: Upon exiting a `try` block, pops the corresponding handler.
- **`EffectCall`**: Triggers an effect, which is essentially a non-local jump that searches the handler stack and jumps to the matching `handle` block.

### 4.3 LIR Phase: VM Primitives
Ultimately, these operations map to core instructions of the `ValkyrieVM`:
- `Raise { effect, resume_target }`: Suspends the current execution flow and searches for a handler.
- `Yield { value, resume_target }`: Used to implement generators or asynchronous operations.
- `PushHandler` / `PopHandler`: Manipulates the `handler_stack` within the `ValkyrieVM` structure.
