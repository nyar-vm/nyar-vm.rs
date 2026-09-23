# CLR (.NET) 后端评估与方案

CLR (Common Language Runtime) 是 .NET 平台的运行时环境。与 JVM 类似，CLR 使用基于栈的指令集（CIL - Common Intermediate Language）。

## 1. 编译流程 (Pipeline)

由于 CLR 也是栈式虚拟机，其编译流程建议参考 JVM 后端的实现，跳过寄存器分配阶段。

- **推荐路径**: `Source -> AST -> HIR -> CFG -> CIL (Common Intermediate Language)`
- **核心逻辑**:
    - **CFG 线性化**: 将 CFG 的块按照顺序排列。
    - **栈指令映射**: 直接从 CFG 表达式生成 `ldloc`, `stloc`, `add`, `call` 等 CIL 指令。
    - **元数据生成**: 使用 `System.Reflection.Emit` 风格的库（或直接操作 PE 格式）生成程序集（Assembly）、类（Class）和方法（Method）的元数据。

## 2. 技术选型

### 方案 A: 静态程序集生成 (AOT-like)
直接生成符合 ECMA-335 标准的 PE 格式二进制文件（.dll 或 .exe）。
- **优点**: 运行不需要编译器存在，性能好。
- **工具**: 
    - [dnlib](https://github.com/0xd4d/dnlib) (C# 库，可能需要 FFI)
    - [Kestrel](https://github.com/jbevain/cecil) (Cecil) 的 Rust 替代品或直接生成二进制流。

### 方案 B: 动态生成 (JIT-like)
在运行时使用反射发射指令。
- **优点**: 实现简单，适合脚本化场景。
- **缺点**: 依赖 .NET 运行时。

## 3. 与 JVM 的差异

1. **值类型 (Struct)**: CLR 原生支持自定义值类型（ValueType），这比 JVM 目前的实现（Project Valhalla 尚在路上）更强大。Valkyrie 的 `struct` 可以直接映射为 CLR 的 `valuetype`。
2. **泛型 (Generics)**: CLR 的泛型是特化（Specialization）的，运行时保留类型信息。这允许 Valkyrie 实现更高效的泛型代码。
3. **尾调用 (Tail Call)**: CIL 显式支持 `tail.` 前缀，非常适合函数式编程语言的优化。

## 4. Valkyrie 特性处理

### Trait 与接口
- **映射方案**: Valkyrie 的 `trait` 可以完美映射到 CLR 的 `interface`。
- **默认实现**: CLR 现在支持接口的默认方法实现，这与 Valkyrie 的 trait 默认实现契合。

### 代数效应 (Algebraic Effects)
- **核心逻辑**: Valkyrie 的 `raise` 和 `yield` 受到 AE 机制管控。
- **异常映射**: 在非 `resume` 情况下，`raise` 等价于 CLR 的 `throw` 指令。需要为不同的效应生成对应的 `Exception` 子类。
- **延续支持 (Continuations)**: 由于 CLR 不支持限定延续，对于 `resume` 场景，需要将函数重写为状态机（类似于 C# 的 `async` 或 `yield return`），将局部变量提升为类字段，并通过状态码管理执行流的恢复。

### FFI 与外部导入 (`@import`)
- **CLR 专用标记**: 针对 `target: clr` 的导入标记，直接映射到 .NET 程序集的元数据。
- **调用方式**: 使用 `call` 指令调用完全限定的方法名（如 `[mscorlib]System.Console::WriteLine`）。
- **独立性**: CLR 后端的 FFI 路径与 WASM/WASI 完全独立，不需要考虑 WASM 目标的调用约定或 Marshalling 垫片。

### 内存管理
- **托管堆**: 直接利用 CLR 的高效分代 GC。
- **析构函数**: Valkyrie 的析构逻辑可以映射到 `IDisposable` 模式。

## 4. 实施进度与计划

### 当前状态 (Current Status)
已实现基于文本的 CIL (Common Intermediate Language) 指令发射，并集成了 `ilasm` 自动化编译流程。
- [x] 基础类型映射 (`int32`, `int64`, `bool`, `string` 等)
- [x] 结构体 (`struct`) 映射到 `valuetype`
- [x] 命名空间支持 (Namespace mapping)
- [x] 代数数据类型 (ADT) 基础映射
- [x] 尾递归优化 (Tail Call Optimization)
- [x] 复杂投影路径加载与存储 (`ldfld`, `stfld`, `ldelema` 等)
- [x] `ilasm` 自动化集成与 EXE 生成
- [x] `main` 函数作为程序入口点 (`.entrypoint`)
- [x] ADT 变体构造函数调用 (Parameterized constructor for ADT)
- [ ] 完整代数效应支持 (Full AE support with state machine)
- [ ] CLR 专用 FFI 导入 (Target-specific FFI for CLR)

### 短期计划 (Short-term Plan)
1. **AE 异常映射**: 实现非 resume 情况下的效应到异常的自动转换。
2. **CLR FFI 验证**: 实现 `@import(target: clr, ...)` 的后端支持，能够成功调用 .NET BCL 方法。
3. **测试框架**: 完善集成测试，支持自动运行生成的 `.exe` 并比对输出。
4. **数组与切片**: 深度测试数组和切片操作的边界情况。

### 长期计划 (Long-term Plan)
1. **泛型支持**: 利用 CLR 原生泛型实现 Valkyrie 泛型。
2. **异常与代数效应**: 探索 `try-catch` 与 Valkyrie 效应系统的映射。
3. **二进制生成**: 考虑引入 PE 生成器以摆脱对 `ilasm` 的依赖。
