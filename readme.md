# Nyar VM

[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-active-success.svg)](#)

## 💡 NyarVM 是什么？

**Nyar VM** 是 Nyar 项目中的一个编译与执行后端，负责自己的 `nyar-ir`、Nyar 字节码、解释器、GC、JIT 与 AOT 链。Nyar 项目可以拥有多个语言前端和多个并列后端；JVM、CLR、Wasm、Native 与 Nyar VM 是不同的目标后端。本文只描述 Nyar VM 后端，不把 `nyar-ir` 当作跨语言或跨后端的 Universal IR。

## 🏗️ 核心架构：三位一体的执行语言

Nyar VM 后端接收某个语言前端为该后端准备的输入 contract，并把它降低为 Nyar VM 自己的 `nyar-ir` 和字节码。语言前端仍拥有自己的语义抽象；其他后端有各自的 lowering、IR 和执行合同，不经过 `nyar-ir`。

```mermaid
graph TD
    %% Global Styles
    classDef frontend fill:#e3f2fd,stroke:#2196f3,stroke-width:2px;
    classDef ir fill:#fff3e0,stroke:#ff9800,stroke-width:2px;
    classDef runtime fill:#f3e5f5,stroke:#9c27b0,stroke-width:2px;
    classDef engine fill:#e8f5e9,stroke:#4caf50,stroke-width:2px;
    classDef os fill:#eceff1,stroke:#607d8b,stroke-dasharray: 5 5;

    subgraph Frontends ["语言前端 (Source Layer)"]
        direction LR
        SL["静态语言 (C, Java, Valkyrie)"]:::frontend
        DL["动态语言 (TS, Python, Lua)"]:::frontend
    end

    subgraph IR_Layer ["Nyar VM 后端内部表示"]
        IR["Nyar VM nyar-ir"]:::ir
    end

    subgraph Runtime ["Nyar VM 后端运行时"]
        direction TB
        VM["VM 解释器 (Quick Start)"]:::runtime
        
        subgraph Acceleration ["加速引擎"]
            JIT["nyar-jit (Dynamic Opt)"]:::engine
            AOT["nyar-aot (Native AOT)"]:::engine
        end
        
        subgraph Management ["资源管理"]
            GC["nyar-gc (TLAB/Block)"]:::runtime
            Types["nyar-types (NaN-Boxing)"]:::runtime
        end
    end

    SL -->|VM backend contract| IR
    DL -->|VM backend contract| IR
    IR --> VM
    VM <--> JIT
    VM <--> AOT
    Runtime <--> OS["Operating System (Win/Lin/Mac/WASI)"]:::os

```

---

## 🏎️ 混合执行策略

### 1. 静态语言流 (Static Languages)

静态语言把已闭合的语言语义交给选定的 Nyar 模块；具体 target 可以在编译或运行阶段选择 specialization、去抽象化与内联。不同语言不必经过相同的 IR 或优化闭包。

```mermaid
graph LR
    classDef source fill:#e3f2fd,stroke:#2196f3;
    classDef process fill:#fff3e0,stroke:#ff9800;
    classDef target fill:#e8f5e9,stroke:#4caf50,font-weight:bold;

    Src["C/Valkyrie 源代码"]:::source -->|VM backend contract| IR["Nyar VM nyar-ir"]:::process
    IR -->|Static Analysis| Plan{"执行决策"}
    
    Plan -->|生产环境| AOT["nyar-aot: Nyar VM AOT 产物"]:::target
    Plan -->|开发阶段| JIT["nyar-jit: 即时优化机器码"]:::target
    Plan -->|极速预览| Interp["VM: 解释执行"]:::target
```

### 2. 动态语言流 (Dynamic Languages)

动态语言可以按自身语义和目标选择 **NaN-Boxing** 等运行时表示；这不是所有 frontend 的共同 IR 或强制执行路径。

```mermaid
graph TD
    classDef source fill:#fce4ec,stroke:#f06292;
    classDef process fill:#fff3e0,stroke:#ff9800;
    classDef hot fill:#f1f8e9,stroke:#8bc34a;

    TS_Src["TypeScript 源代码"]:::source -->|VM backend contract| IR["Nyar VM nyar-ir"]:::process
    IR -->|VM| Profile["运行时类型采样"]:::process
    Profile -->|收集 Tag| Guard{"类型守卫消除"}
    Guard -->|Hot Path| JIT["JIT 特化机器码"]:::hot
    IR -->|Snapshot| AOT_Snap["AOT 二进制快照"]:::hot
```

#### NaN-Boxing 64-bit 内存布局

所有动态类型（指针、整数、布尔）均压缩在 8 字节字宽内，零开销识别。

```mermaid
packet-beta
title NaN-Boxing Value Layout
0-12: "NaN Base (All 1s)"
13-16: "Tag (4 bits)"
17-63: "Payload (47 bits: GC Ptr / i47 / bool)"
```

---

## 🧩 核心基础设施

### 1. 代数效应 (Algebraic Effects)

`nyar-ir` 原生支持高阶控制流；Nyar VM 通过捕获 **Continuation** 实现异步与协程。这是 Nyar VM 后端的能力，不是所有后端的共同 IR 要求。

```mermaid
sequenceDiagram
    autonumber
    participant Op as Nyar Opcode
    participant VM as VM Runtime
    participant Handler as Effect Handler
    
    Op->>+VM: Perform(Effect)
    Note over VM: Capture Current Stack<br/>(Continuation)
    VM->>+Handler: Jump to Nearest Handler
    Handler->>-VM: Process & Resume
    VM->>-Op: Restore Stack & Continue
```

### 2. 内存管理 (nyar-gc)

专为 Nyar 执行模型设计的垃圾回收器，支持线程本地分配缓存 (TLAB)。

```mermaid
graph LR
    classDef heap fill:#e1f5fe,stroke:#01579b;
    classDef thread fill:#f1f8e9,stroke:#33691e;
    classDef block fill:#ffffff,stroke:#0288d1,stroke-width:2px;

    subgraph GC_Heap ["分块式堆空间 (Global Heap)"]
        direction TB
        Block1["Block 0 (Used)"]:::block
        Block2["Block 1 (Active)"]:::block
        BlockN["Block N (Free)"]:::block
    end
    
    subgraph Threads ["工作线程 (Worker Threads)"]
        T1["Thread 1"]:::thread -->|TLAB| Block1
        T2["Thread 2"]:::thread -->|TLAB| Block2
    end

    class GC_Heap heap
```

---

## 🏗️ 项目模块导航

| 模块           | 路径                                                                           | 功能描述                                |
|:---------------|:-------------------------------------------------------------------------------|:----------------------------------------|
| **虚拟机核心** | [`odin-vm`](./projects/runtimes/odin-vm)                                       | 字节码解释器、异步运行时及驱动          |
| **类型系统**   | [`nyar-types`](./projects/compilers/nyar-types)                                | NaN-Boxing 实现与核心类型定义           |
| **垃圾回收**   | [`nyar-gc`](./projects/runtimes/nyar-gc)                                       | 高性能分块式垃圾回收器                  |
| **加速引擎**   | [`nyar-jit`](./projects/runtimes/nyar-jit) / [`nyar-aot`](./projects/nyar-aot) | 动态与静态编译优化工具链                |
| **示例前端**   | [`examples/`](./examples)                                                      | C, Java, Go, TS 等多种语言的 Nyar 实现  |
| **语言主链**   | [`rusty-valkyrie`](./examples/rusty-valkyrie)                                    | Valkyrie 前端：`HIR` / `MIR` 与语义闭包 |

Per-crate docs live in each directory's `readme.md` (for example [
`odin-vm/readme.md`](./projects/runtimes/odin-vm/readme.md)).

---

## 🛠️ 快速开始

```powershell
# 构建全量发布版本
cargo build --release

# 运行基准测试
cargo bench -p odin-vm

# 执行 Nyar 字节码程序
cargo run -p odin-vm -- <input_file>
```

## 📖 深入探索

- [Nyar IR 指令集详情](./documentation/zh-hans/maintenance/nyar-ir.md)
- [对象级动态技术](./documentation/zh-hans/guide/dynamic.md)
- [NaN-Boxing 系统深度解析](./documentation/zh-hans/maintenance/nyar-types.md)
