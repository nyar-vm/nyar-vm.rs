# Rusty Python

Rusty Python 展示了如何将具有复杂动态特性的 Python 映射到 Nyar VM 架构。

## 核心特性

- **面向对象**: 完整的类定义、多重继承、方法解析顺序（MRO）支持。
- **高级语法**: 列表/字典推导式、解构赋值、装饰器原型。
- **异常处理**: 基于栈展开的 `try-except-finally` 机制。
- **模块系统**: 基于命名空间的 `import` 与 `from ... import ...` 支持。

## 编译与 Lowering 流程

Rusty Python 提供两条独特的编译路径：

1. **Nyar 原生路径**:
   - **解析**: 使用 `oak-python` 生成 UIR。
   - **Lowering**: 通过 `GaiaTranslator` 转换为 `Gaia` 指令，利用 Nyar VM 的动态属性优化。
2. **PYC 兼容路径**:
   - **字节码发射**: 使用 `pyc_codegen.rs` 将 Python AST 直接发射为 Python 3.10 兼容的操作码。
   - **序列化**: 使用 `Marshal` 模块将 `PyCodeObject` 序列化为标准的 `.pyc` 二进制格式。

## 对象模型处理

Python 的高度动态性在 Nyar VM 中通过以下方式处理：

- **动态属性**: 对象属性存储在 `__dict__` 字典中，通过 VM 的 `LoadAttr` 和 `StoreAttr` 指令访问。
- **类型层级**: 模拟 Python 的 `type` 和 `object` 关系，支持运行时的类型检查与转换。
- **作用域**: 区分 `LoadFast`（局部变量）、`LoadName`（普通变量）和 `LoadGlobal`（全局变量）。

## Builtin 与 FFI 实现

- **内建对象**: 映射 Nyar VM 的基础类型到 Python 的 `int`, `str`, `list`, `dict` 等。
- **FFI 交互**: 
   - 支持通过 `ctypes` 样式的接口调用 C 函数。
   - 能够加载 Python 官方生成的 `.pyc` 文件并在 Nyar VM 中执行其逻辑。
- **优化策略**: 针对常用的内建函数（如 `len()`, `range()`）提供 VM 层面的快速路径（Fast Path）。
