# Rusty C#

Rusty C# 示例侧重于 .NET 运行时的特性映射，包括属性、事件和泛型基础。

## 核心特性

- **属性与索引器**: 模拟 C# 的 `get`/`set` 访问器逻辑。
- **委托与事件**: 基于函数引用与多播机制的事件系统原型。
- **值类型优化**: 结构体（Struct）的内存分配优化，减少 GC 压力。
- **命名空间**: 完善的程序集（Assembly）与命名空间管理模拟。

## 编译与 Lowering 流程

Rusty C# 的编译路径与 Java 类似，但增加了对 C# 特有语法的处理：

1. **解析阶段**: 使用 `oak-csharp`（或兼容层）生成红绿树结构的 AST。
2. **Lowering 逻辑**:
   - 将属性访问转换为隐式的方法调用（`get_Property` / `set_Property`）。
   - 事件绑定映射为对内部委托列表的更新操作。
3. **UIR 增强**: 
   - 使用 `Extension` 节点描述 C# 特有的元数据，如 `[Attribute]`。
   - 泛型定义在 UIR 层面保留原始信息，支持运行时实例化。

## 对象模型处理

- **统一类型系统**: 模拟 .NET 的 `System.Object` 根类模型。
- **装箱与拆箱**: 处理值类型与引用类型转换时的指令序列生成。
- **虚方法调用**: 区分 `call` (非虚调用) 与 `callvirt` (虚调用) 的实现策略。

## Builtin 与 FFI 实现

- **P/Invoke 模拟**: 
   - 通过 `[DllImport]` 样式的元数据触发 FFI 流程。
   - 自动生成数据封送（Marshaling）代码。
- **内建类型**: 映射 Nyar VM 的基础类型到 `System.Int32`, `System.String` 等标准名称。
- **异常处理**: 实现 `try-catch-finally` 到 Nyar VM `ExceptionHandler` 的映射。
