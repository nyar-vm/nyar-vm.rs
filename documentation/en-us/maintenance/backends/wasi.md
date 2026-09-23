# WASM 后端维护指南 (WASI Preview 2)

WASM 后端负责将 Valkyrie 编译为 WebAssembly 组件 (Component) 格式，遵循 WASI Preview 2 标准。

## 1. 编译流水线

`Source -> AST -> HIR -> CFG -> WASM Component`

目前的实现直接从 `CFG` 生成 `WASM` 指令，并封装为 `WebAssembly Component`。

## 2. 设计考量

### 栈机架构与控制流
- **CFG 线性化**: 使用 `$dispatch` 循环和 `br_table` 实现非结构化控制流到 WASM 结构化控制流的映射。
  - **Relooper 结构**: 采用嵌套 `block` 结构。每个 CFG 基本块对应一个 `block`。
  - **调度逻辑**: 使用一个局部变量 `$dispatch_local` 存储下一个要执行的块索引。
- **局部变量**: Cfg 中的 Local 映射为 WASM 的 `local`。Local 0 约定为返回值。

### 类型与运算
- **类型映射**: `CfgType` 映射为 WASM `ValType`。
  - 指针、引用、数组、结构体等在 WASM 线性内存中统一映射为 `i32` (wasm32) 或 `i64` (wasm64)。
- **算术运算**: 根据操作数类型自动选择指令（如 `i64.add`, `f64.add`）。目前默认整数运算使用 `i64`。
- **常量池**: 字符串常量在编译时收集，存储在 WASM `DataSection` 中，运行时通过偏移量访问。

### 内存布局与对齐
- **线性内存**: 采用 `wasm-encoder` 构建。
- **结构体布局**: 
  - 字段按照定义的顺序排列。
  - **内存对齐**: 每个字段根据其类型的自然对齐要求进行对齐（如 `i64` 对齐到 8 字节）。
  - **总大小**: 结构体总大小对齐到其成员中的最大对齐值。
- **枚举布局**: 
  - 采用 `Tag + Payload` 模式。
  - Tag 为 `i32`，位于偏移量 0。
  - Payload 紧随其后，并根据成员的最大对齐要求进行对齐。
  - **变体字段**: 支持带有字段的枚举变体，通过组件模型的 `variant` 类型进行映射。
- **内存管理**:
  - **堆分配**: 核心模块实现了 `cabi_realloc` 函数，遵循 Canonical ABI 标准。
  - **分配算法**: 目前采用简单的 Bump Allocation (指针碰撞) 算法，适用于短期运行或小型脚本。
  - **堆指针**: 使用 WASM 全局变量 (Global) 跟踪当前堆顶，初始位置紧跟在 `DataSection`（常量池）之后。

### 组件模型 (Component Model / WASI P2)
- **多模块架构**: 
  - `MockMemory`: 负责导出线性内存，作为组件内部的单一事实来源。
  - `Main`: 核心逻辑模块，导入 `MockMemory` 导出的内存，并导出 `cabi_realloc` 用于内存管理。
- **接口对接 (Docking)**:
  - **wasi:cli/stdout**: 已实现接口导入，支持通过 `get-stdout` 获取标准输出句柄。
  - **wasi:io/streams**: 已实现 `write` 接口对接，支持向输出流写入字节序列。
  - **Canonical ABI**: 
    - 使用 `canon lower` 将组件级别的函数（如 `write`）降低为核心模块可调用的函数。
    - 降低过程关联了 `MockMemory` 提供的内存，以支持 `list<u8>` (string) 类型的传递。
- **实例化与链接**: 使用 `ComponentInstanceSection` 和 `ComponentAliasSection` 在组件内部完成模块的实例化和链接。目前已支持多层级的别名映射，确保核心模块能正确识别并调用降低后的 WASI 函数。

## 3. Valkyrie 特性处理

WASM 后端通过 `WasmConfig` 进行配置：

- **Variant**: 支持 `wasm32` 和 `wasm64`。
  - `wasm32`: 使用 32 位地址空间，这是目前的主流选择。
  - `wasm64`: 使用 64 位地址空间，适用于需要大内存支持的场景。
- **Effect Lowering**:
  - `experimental_stack_switch`: 布尔值。若为 `true`，则尝试使用 WASM 原生的 `stack-switching` 提案（方案 B）；若为 `false`，则回退到兼容性更好的 CPS 变换（方案 C）。

## 2. Valkyrie 特性处理

### Trait 与多态
- **实现方案**: 采用经典的 VTable (Virtual Method Table) 方案。
- **内存布局**: 对象头包含一个指向线性内存中 VTable 的偏移量。VTable 存储函数索引 (Function Index)。
- **调用方式**: 使用 `call_indirect` 指令根据 VTable 中的索引动态调用函数。

### 代数效应 (Algebraic Effects)
Valkyrie 的核心特性之一是代数效应，在 WASM 中的实现具有挑战性。随着浏览器对新提案的支持，目前规划如下：
- **方案 A (Asyncify)**: 利用 Binaryen 的 `asyncify` 工具在用户态保存和恢复调用栈。（不再作为首选方案）
- **方案 B (Stack Switching)**: 利用 WASM 原生的 `stack-switching` 提案。这是实现可恢复（resumable）效应的最优路径。可通过 `experimental_stack_switch = true` 开启。
- **方案 C (CPS 变换)**: 在编译阶段将带有效应的代码转换为续体传递风格 (Continuation Passing Style)。这是默认方案 (`experimental_stack_switch = false`)。

**当前状态与评估**: 
- `Raise`: 
    - **非恢复路径 (Non-resumable)**: 鉴于 WASM `exception-handling` 提案已在浏览器实装，我们将优先使用 `throw` 指令实现 `Raise`。这使得效应在不恢复时表现为标准异常。
    - **可恢复路径**: 依赖 `stack-switching` 或 `CPS` 变换。
- `PushHandler` / `PopHandler`: 需要结合 `try-catch` 或 `try_table` 指令实现。

### 内存管理 (Memory Management)
- **线性内存模型**: 目前实现了一个极简的 Bump Allocator (`cabi_realloc`)。由于缺乏 `free`，仅适用于短期任务。
- **WASM GC 模型**: 鉴于 WASM GC 提案已实装，目前已引入基于 GC 对象的类型表达支持。
    - **当前进度**: 
        - 结构体和数组已支持映射为 WASM 的 `struct` 和 `array` 类型。
        - 已实现基于 `struct.new` / `array.new_fixed` 的对象分配。
        - 已实现基于 `struct.get` / `struct.set` 和 `array.get` / `array.set` 的字段与索引访问。
    - **优势**: 消除内存泄漏，增强安全性，并简化 AE 续体中的对象生命周期管理。
- **建议**: 并行保留线性内存模型（用于底层 FFI）和 GC 模型（用于 Valkyrie 原生类型）。可通过 `experimental_gc` 配置项开启。

### 复杂类型支持
- **聚合类型 (Struct/Array)**:
    - 基础分配已实现，但 `emit_load` / `emit_store` 尚不支持聚合类型的按值拷贝（Memcpy）。
    - 数组字面量在 `AST -> HIR` 阶段存在降级丢失问题。
- **字符串 (String)**: 将对接 WASI 的组件模型字符串表达，直接在 WASM 二进制中编码相关的类型定义。

### 组件模型与工具链 (Component Model)
- **直接构建**: 我们不依赖 `wit-component` 等外部工具。WASI Preview 2 所需的组件包装、类型声明（如 `WIT` 对应的部分）均通过直接写入 WASM 二进制（Component Section）的形式实现。
- **断裂点**: `AST -> HIR` 和 `HIR -> CFG` 阶段对某些复杂表达式（如数组、闭包）的处理存在缺失。
- **验证**: 持续通过 `wasi_test.rs` 跟踪修复进度。目前 `arithmetic` 通过，`struct/array/control_flow` 仍受限于上述缺失特性。

## 4. 剩余缺失特性与待办事项 (Missing Features & Roadmap)

### 核心功能
- [ ] **代数效应 (AE)**:
    - [ ] 实现 `Raise` 的非恢复路径（映射至 WASM `throw` 指令）。
    - [ ] 实现 `PushHandler` / `PopHandler`（映射至 WASM `try-catch` 或 `try_table`）。
    - [ ] 研究并实现 `stack-switching` 提案下的续体恢复逻辑。
- [ ] **Trait 与多态**:
    - [ ] 设计并实现线性内存中的 VTable 布局。
    - [ ] 实现基于 `call_indirect` 的动态分发。
- [ ] **枚举 (Enum)**:
    - [ ] 实现 `Tag + Payload` 的线性内存布局。
    - [ ] 支持 GC 模式下的变体表达（可能映射至 WASM `struct` 的子类或联合）。

### 优化与增强
- [ ] **GC 模式完善**:
    - [ ] 支持 GC 数组的动态长度分配 (`array.new`)。
    - [ ] 支持 GC 字符串 (`string.new_utf8` 等)。
    - [ ] 实现 GC 对象与线性内存 FFI 的桥接层。
- [ ] **后端架构**:
    - [ ] 增强 `relooper` 逻辑以支持更复杂的控制流（如带标签的 `break`）。
    - [ ] 实现 `memcpy` 优化，用于聚合类型的线性内存按值拷贝。

### 工具与验证
- [ ] **测试覆盖**:
    - [ ] 修复 `AST -> HIR` 阶段对数组、结构体构造函数的处理缺失。
    - [ ] 增加更多针对 GC 模式和 AE 机制的单元测试。