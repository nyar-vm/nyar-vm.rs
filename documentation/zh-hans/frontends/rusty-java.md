# Rusty Java

Rusty Java 专注于展示 Java 虚拟机（JVM）风格的面向对象模型在 Nyar VM 中的实现。

## 核心特性

- **类与接口**: 支持严格的类定义、接口实现与单继承模型。
- **类型系统**: 包含基础类型（int, boolean）及其包装类映射。
- **多态性**: 支持方法重载（Overloading）与虚方法重写（Overriding）。
- **内存管理**: 配合 `nyar-gc` 实现基于可达性分析的垃圾回收验证。

## 编译与 Lowering 流程

Rusty Java 采用结构化的 AST 到 UIR 转换策略：

1. **解析**: 使用 `oak-java` 将 `.java` 源码解析为高阶 AST。
2. **UIR 转换 (Lowering)**:
   - 使用 `NyarTranslator` 将 AST 节点转换为 `IKunTree`。
   - **类结构**: 映射为 `IKunTree::Extension` 节点（如 `"class"`, `"method"`, `"field"`）。
   - **方法体**: 递归降低为基础意图（Intents），如 `Constant`, `Symbol`, `Call`。
3. **字节码生成**:
   - `NyarBackend` 遍历 `IKunTree`，发射 Nyar VM 操作码。
   - 方法被封装为 `Chunk`，并添加到 `NyarModule` 的导出表中。

## 对象模型处理

在 Nyar VM 中，Java 的对象模型被映射为动态扩展的对象结构：

- **虚方法表 (VTable)**: 通过 `InvokeMethod` 指令实现。VM 在运行时根据对象的实际类型查找对应的 `Chunk`。
- **字段访问**: 使用 `GetField` 和 `SetField` 指令，基于字符串名称或索引进行成员变量访问。
- **构造函数**: 映射为特殊的静态方法，负责对象分配（Allocate）与初始化。

## Builtin 与 FFI 实现

- **内建库**: 模拟 `java.lang` 包，提供 `System.out.println` 等核心 API 的内建实现。
- **符号解析**: 
   - 使用 `CallSymbol` 访问全局函数。
   - 通过 `ImportInfo` 管理跨包/跨模块的类引用。
- **FFI 支持**: 允许通过 `native` 关键字声明并链接到 C 语言编写的底层实现。
