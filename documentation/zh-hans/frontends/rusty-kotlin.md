# Rusty Kotlin

Rusty Kotlin 展示了 Kotlin 语言的现代特性如何通过 Nyar VM 的异步与 Effect 系统实现。

## 核心特性

- **空安全**: 编译时类型检查与运行时的非空断言生成。
- **扩展函数**: 静态解析扩展函数，将其 Lowering 为带接收者参数的普通函数。
- **数据类**: 自动生成 `equals`, `hashCode`, `toString` 以及 `copy` 方法的 UIR 表达。
- **异步支持**: 协程原语到 Nyar VM Effect 系统的映射。

## 编译与 Lowering 流程

Rusty Kotlin 利用了 Nyar VM 的高级指令集特性：

1. **解析**: 使用 `oak-kotlin` 生成 AST。
2. **Lowering 转换**:
   - **扩展属性/函数**: 将 `A.foo()` 转换为 `foo(A)`，并在符号查找时进行特殊处理。
   - **Lambda 表达式**: 转换为闭包对象，支持变量捕获。
3. **Effect 系统集成**:
   - 协程的 `suspend` 关键字映射为 Nyar VM 的 `yield` 或 `await` 语义。
   - 状态机生成逻辑在 UIR 层面通过控制流重写实现。

## 对象模型处理

- **属性委托**: 将属性访问重定向到委托对象的 `getValue`/`setValue` 方法。
- **伴生对象 (Companion Objects)**: 映射为类关联的单例对象。
- **类型擦除**: 在 Lowering 过程中处理泛型擦除，并在需要时生成 `checkcast` 指令。

## Builtin 与 FFI 实现

- **Kotlin 标准库模拟**: 
   - 提供 `List`, `Map` 等集合类型的内建实现。
   - 映射 `Int`, `String` 等到 Nyar VM 原生类型。
- **互操作性**: 
   - 支持调用 Rusty Java 定义的类。
   - 通过 `external` 关键字支持外部函数链接。
- **Effect 实现**: 协程调度器作为 VM 的内建插件（Plugin）实现。
