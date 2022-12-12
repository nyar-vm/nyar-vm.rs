# nyar-vm

`nyar-vm` 是整个 Nyar 生态系统的核心，提供了一个基于操作数栈的高效字节码执行引擎。

## 核心职责

- **字节码规范**: 定义 `Nyarc` 二进制模块格式。
- **执行引擎**: 实现基于栈的解释器。
- **内存管理**: 基于 **Nyar GC**，支持 RAII 语义与终结器 (Finalizer) 机制，确保资源安全。
- **异步支持**: 原生协程与 `Await` 机制。
- **代数效应 (Effect System)**: 提供比异常处理更强大的控制流抽象。

## 模块格式 (Nyarc)

`.nyar` 文件由以下部分组成：
- **Header**: `NYAR` 魔数、版本号、标志位、时间戳。
- **Constant Pool**: 存储整数、浮点数、字符串常量。
- **Chunks**: 包含代码字节、局部变量数、最大栈深度、上值信息。
- **Metadata**: 包含类（Classes）、接口（Traits）、实现（Impls）、导入（Imports）与导出（Exports）信息。

## 核心指令集概览

| 类别 | 指令示例 | 描述 |
| :--- | :--- | :--- |
| **栈操作** | `Push`, `Pop`, `Dup`, `Swap` | 基础操作数栈管理 |
| **变量存取** | `LoadLocal`, `StoreGlobal`, `LoadUpvalue` | 处理不同作用域的变量 |
| **控制流** | `Jump`, `JumpIfFalse`, `Call`, `Return` | 改变执行路径 |
| **面向对象** | `NewObject`, `GetField`, `InvokeMethod` | 类与实例操作 |
| **扩展类型** | `I32Ext`, `F64Ext`, `StringExt` | 针对特定类型的优化指令 |
| **代数效应** | `Perform`, `WithHandler`, `ResumeWith` | Effect 的触发与处理 |
| **元编程** | `Quote`, `Splice`, `Eval` | 运行时代码生成与求值 |

## 解释器循环

解释器通过一个高效的 `match` 循环（或在 JIT 模式下生成的跳转表）执行指令：

```rust
// 伪代码示例
loop {
    let opcode = self.fetch_u8();
    match opcode {
        Opcode::Push => {
            let idx = self.fetch_u16();
            let val = self.module.constants[idx].clone();
            self.stack.push(val);
        }
        // ... 其他指令
        Opcode::Halt => break,
    }
}
```

## 见证表与动态分发实现 (Witness Table)

Nyar VM 通过 **见证表 (Witness Table)** 实现了高效的运行时多态（Trait Objects）。

### 为什么叫“见证”？

从类型论的角度看，Trait 对象是一种 **存在量化类型 ($\exists T. P(T)$)**。当你持有一个 Trait 对象时，你只知道“存在某个类型 $T$ 实现了该 Trait”，但具体的 $T$ 对你来说是不可知的（被擦除）。

此时，`WitnessTable` 扮演了“见证者”的角色：它包含了证明该类型确实满足 Trait 约束的所有信息，并指导虚拟机如何 **打开 (Open)** 这个不透明的存在类型，从而调用具体的方法。

### 数据结构

在 `Value` 系统中，`WitnessTable` 被定义为一个包含模块索引和方法列表的结构：

- **module_idx**: 该实现所属的模块索引。
- **methods**: 一个包含 `Chunk` 索引的列表，按 Trait 定义中方法的顺序排列。

### 核心指令

见证表的操作依赖于两条关键字节码指令：

1.  **`GetWitnessTable(class_idx, trait_idx)`**:
    - **原理**: 虚拟机在运行时扫描模块的 `Metadata` 中的 `ImplInfo` 列表。
    - **逻辑**: 查找匹配给定 `class_idx` 和 `trait_idx` 的实现块。
    - **输出**: 如果找到，则创建一个 `WitnessTable` 对象并推入栈顶。
2.  **`WitnessMethod(table, method_idx)`**:
    - **原理**: 从见证表中提取具体的方法实现。
    - **逻辑**: 根据 `method_idx` 从表的 `methods` 列表中取出对应的 `Chunk` 索引。
    - **输出**: 将该方法包装为一个 `Closure`（闭包）推入栈顶，随后可以使用标准的 `Call` 指令进行调用。

### GC 集成

`WitnessTable` 实现了 `Trace` trait，这意味着：
- 它受到 Nyar GC 的统一管理。
- 在垃圾回收的标记阶段，它会确保其关联的方法索引和模块信息是可达的。
- 保证了在动态分发过程中的内存安全，防止调用已释放的代码块。

### 实现优势

- **解耦 (Decoupling)**: 虚拟机不需要在对象布局中预留虚表指针。这种非侵入式设计允许为已编译的模块添加新的接口实现。
- **内存效率**: 相比于为每个对象实例存储一个 vtable 指针，见证表仅在需要动态分发时由“胖指针”携带，显著减少了大量小对象的内存开销。
- **二进制兼容性**: 增加或修改 Trait 实现不会改变类的内存布局，极大地简化了热重载和跨模块调用的 ABI 稳定性。
- **高性能查找**: 见证表是扁平且连续的，`WitnessMethod` 指令仅需一次偏移量读取即可获得方法入口点，性能接近于直接虚函数调用。

### 见证表与传统 vtable 的底层差异

| 维度 | 传统 vtable (C++/Java) | Nyar VM 见证表 |
| :--- | :--- | :--- |
| **存储位置** | 存储在对象实例的头部或尾部。 | 存储在运行时产生的“胖指针”或寄存器中。 |
| **构建时机** | 编译时静态确定类继承树。 | 运行时通过模块元数据动态构建或链接。 |
| **查找机制** | `obj -> vptr -> vtable -> method` | `witness_table -> method_list[idx]` |
| **灵活性** | 单一继承或复杂的偏移量多继承。 | 每个 Trait 拥有独立的、扁平的见证表。 |

Effect 系统允许开发者定义抽象的副作用（如 `IO`, `Log`, `State`），并在调用栈的高层进行统一处理。
1.  **Perform**: 触发一个效应，挂起当前上下文。
2.  **WithHandler**: 建立一个处理器上下文。
3.  **ResumeWith**: 处理器处理完后，将结果返回给被挂起的上下文并继续执行。
