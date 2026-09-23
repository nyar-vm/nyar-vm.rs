# Rusty Scheme

`Rusty Scheme` 是 Nyar VM 对经典 Lisp 方言的实现，主要用于验证 VM 对**函数式编程**和**高级控制流抽象**（如一级延续）的支持能力。

## 核心特性

- **一级延续 (call/cc)**: 映射到 Nyar VM 的 `Continuation` 类型，支持非局部跳转与状态恢复。
- **尾调用优化 (TCO)**: 利用 Nyar VM 的跳转指令实现零栈消耗的无限递归。
- **宏系统**: 支持语法转换，映射到 VM 的自托管编译期执行。
- **完全动态类型**: 所有值均为 `Value` 类型，利用 NaN-Boxing 实现高效存储。

## 核心验证目标

- **延续捕获 (call/cc 测试)**: 
    - 验证 VM 是否能完整捕获当前执行栈并多次激活。
    - 证明 Nyar VM 是一个可以支撑“不可思议”控制流的现代运行时。

## 编译与 Lowering 流程

1. **S-Expression 解析**: 将源码解析为嵌套的列表结构。
2. **CPS 变换**: 在 Lowering 阶段将代码转换为延续传递风格（Continuation Passing Style），以便于映射到 VM 的控制流指令。
3. **闭包构造**: 所有 lambda 表达式降级为带环境捕获的闭包对象。

## 实现进度

- [ ] S-Expression 解析器
- [ ] 基础闭包与函数调用
- [ ] 尾调用优化 (TCO)
- [ ] call/cc 语义支持 (一级延续测试)
