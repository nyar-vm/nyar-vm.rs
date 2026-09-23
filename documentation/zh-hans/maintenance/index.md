# 核心项目维护指南

Nyar VM 采用了高度模块化的架构设计，旨在构建一个跨语言、跨平台的通用运行时环境。其核心理念是通过 `ProjectChomsky` 的 `UIR`（Universal IR）作为中转站，实现“一次编写，多处编译/执行”。

## 架构总览

Nyar VM 的生态系统主要分为三层：

1.  **前端层 (Frontends)**: 各类编程语言前端（如 Mini-C, Mini-Java 等），负责将源码解析为 AST 并逐步降级。
2.  **中间层 (Infrastructure)**:
    -   **nyar-types**: HIR, CFG, SSA, LIR 的核心定义。
    -   **nyar-gc**: 托管堆内存管理。
    -   **ProjectChomsky**: 核心优化引擎（基于 E-Graph 的普遍优化）。
3.  **执行层 (Execution)**:
    -   **nyar-vm**: 栈式字节码解释器与异步运行时。
    -   **nyar-jit**: 基于热点分析的即时编译。
    -   **nyar-aot**: 静态编译驱动，负责从 SSA/LIR 到目标构件（Native/WASM/JVM）的转换与优化。
    -   **nyar-tools**: 开发者工具链。

## 项目调用关系

```mermaid
graph TD
    Tools[nyar-tools] --> VM[nyar-vm]
    Tools --> AOT[nyar-aot]
    Tools --> JIT[nyar-jit]
    
    VM --> GC[nyar-gc]
    VM --> Types[nyar-types]
    
    JIT --> Chomsky[ProjectChomsky]
    AOT --> Chomsky
    
    Frontends[Language Frontends] --> Types
    Types --> Chomsky
```

## 编译流水线 (Pipeline)

Nyar 遵循 **渐进式下放 (Progressive Lowering)** 原则：

```mermaid
graph TD
    A[Source Code] -->|Parser| B(AST: Syntax Tree);
    B -->|Semantic| C(HIR: Semantic Graph);
    C -->|Linearize| D(CFG: Control Flow Graph);
    D -->|SSA Transform| E(SSA: Static Single Assignment);
    
    subgraph Optimization [Chomsky Universal Optimization]
        E <-->|Lifting / Lowering| U(UIR: Universal IR / Intents);
        U -->|Equality Saturation| U;
    end
    
    E -->|Lowering| F(LIR: Low-level Stack Machine);
    F -->|Emit| G[WASM / Bytecode];
```

- **Chomsky 普遍优化**: 在 SSA 阶段介入，利用 E-Graph 引擎在等价空间中搜索最优执行路径，提取出针对 `nyar-vm` 成本模型的最优实现。
- **SSA -> LIR**: 销毁 SSA 结构，消除 Phi 节点，映射到紧凑的栈式指令集。

## 项目目录

- [nyar-vm](nyar-vm.md): 核心运行时、字节码定义与解释器。
- [nyar-types](nyar-types.md): 通用基础类型与错误处理。
- [nyar-gc](nyar-gc.md): 垃圾回收器实现。
- [nyar-jit](nyar-jit.md): 即时编译器驱动。
- [nyar-aot](nyar-aot.md): 静态编译器驱动。
- [nyar-tools](nyar-tools.md): 命令行工具链。
