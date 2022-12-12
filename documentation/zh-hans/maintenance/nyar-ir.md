# nyar-ir

`nyar-ir` 定义了 Nyar VM 的指令集架构。它是一种基于栈（Stack-based）的字节码，设计上兼顾了动态语言的灵活性与高性能执行的需求。

## 指令集概览

Nyar 字节码指令分为若干逻辑组，涵盖了从基础栈操作到高级代数效应处理的所有功能。

### 1. 基础栈操作
- `Push`: 将常量压入栈。
- `Pop`: 弹出栈顶元素。
- `Dup`: 复制栈顶元素。
- `Swap`: 交换栈顶两个元素。

### 2. 变量访问
- `LoadLocal` / `StoreLocal`: 访问函数局部变量。
- `LoadGlobal` / `StoreGlobal`: 访问模块全局变量。
- `LoadUpvalue` / `StoreUpvalue`: 访问闭包捕获的变量（Upvalues）。
- `CloseUpvalues`: 关闭特定范围内的 Upvalues。

### 3. 控制流
- `Jump`: 无条件跳转。
- `JumpIfFalse`: 条件跳转（当栈顶为 false 时）。
- `JumpIfNull`: 条件跳转（当栈顶为 null 时）。
- `Return`: 从当前函数返回。
- `Halt`: 停止 VM 执行。

### 4. 调用与分发
- `Call`: 直接调用。
- `CallVirtual`: 虚函数调用。
- `CallDynamic`: 动态分发调用。
- `CallClosure`: 调用闭包。
- `InvokeMethod`: 调用对象方法。
- `CallSymbol`: 通过符号名调用。
- `FFICall`: 外部函数接口调用。
- `TailCall`: 尾调用优化。

### 5. 对象与数据结构
- `NewObject`: 创建新对象。
- `GetField` / `SetField`: 访问对象字段。
- `NewArray` / `NewList` / `MakeTuple`: 创建容器类型。
- `GetElement` / `SetElement`: 索引访问。
- `MatchVariant`: 枚举变体匹配。
- `NewDynObject`: 创建动态对象（字典）。

### 6. 代数效应与协程 (Effect System)
Nyar VM 的核心特性之一是对代数效应的原生支持：
- `Perform`: 触发一个 Effect。
- `WithHandler`: 绑定 Effect 处理器。
- `ResumeWith`: 恢复延续（Continuation）。
- `CaptureCont`: 捕获当前执行上下文作为延续。
- `Await`: 异步等待。

### 7. 类型扩展指令 (Extensions)
为了提升数值运算和字符串处理性能，Nyar 定义了专门的扩展指令集：
- `I32Ext` / `I64Ext`: 整数算术、比较及位运算。
- `F32Ext` / `F64Ext`: 浮点数运算。
- `BigIntExt`: 高精度整数运算。
- `StringExt`: 字符串拼接、子串提取及长度计算。

## 编码格式

每条指令通常由 1 字节的操作码（Opcode）开头，后跟可选的操作数（Operands）。对于扩展指令，操作码后紧跟一个子操作码（Sub-opcode）。

例如，一个 I32 加法指令在二进制中表现为：
`0xC1 (I32Ext) | 0x01 (Add)`

这种多级指令设计允许我们在不耗尽单字节操作码空间的前提下，支持极大规模的专业指令集。
