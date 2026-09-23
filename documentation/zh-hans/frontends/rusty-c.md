# Rusty C

Rusty C 是一个简化版的 C 语言前端实现，专注于展示过程化编程语言在 Nyar VM 上的映射。

## 核心特性

- **基础数据结构**: `int`, `char`, `void` 指针等基础类型映射。
- **控制流**: 完整的 `if`, `while`, `for`, `switch` 支持。
- **函数模型**: 支持函数定义、递归调用以及外部函数调用（FFI）。
- **预处理器**: 简单的宏替换与头文件包含模拟。

## 编译与 Lowering 流程

Rusty C 的编译流程遵循从源码到 UIR 再到原生指令的路径：

1. **词法/语法分析**: 使用 `oak-c` 解析器将源码转换为 `RedNode` 树（红绿树架构）。
2. **UIR 生成**: 
   - 遍历 `RedNode` 树，通过 `IntentBuilder` 构建 `IKun` 意图流。
   - 函数定义被映射为 `builder.function`，变量声明映射为 `builder.state_update`。
   - 控制流通过 `builder.branch` 和 `builder.jump` 实现。
3. **优化 (Chomsky)**: 
   - 使用 `EGraph` 进行等价类重写。
   - 应用常量折叠、死代码消除等过程化优化策略。
4. **后端发射**: 
   - **JIT 模式**: 通过 `Gaia` 后端生成机器码。
   - **AOT 模式**: 通过 `NyarAot` 生成原生二进制文件。

## 对象模型处理

虽然 C 是过程化语言，但在 Nyar VM 中其对象模型按以下方式处理：

- **内存布局**: 局部变量映射到 VM 的栈帧（Stack Frame）或寄存器。
- **符号管理**: 函数名作为全局符号导出，支持跨模块链接。
- **指针模型**: 模拟 Nyar VM 的引用与原始指针转换，确保安全边界内的内存访问。

## Builtin 与 FFI 实现

- **内建函数**: 如 `printf`, `malloc` 等通过 `NyarRuntime` 预置的符号表实现。
- **FFI 机制**: 
   - 使用 `extern "C"` 声明。
   - 通过 Nyar VM 的 `ForeignFunction` 接口加载动态库并进行参数封送（Marshaling）。
   - 支持基本类型（int, float）与指针类型的自动转换。
