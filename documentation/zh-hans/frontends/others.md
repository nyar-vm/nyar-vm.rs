# 其他语言前端

除了上述核心语言外，Nyar VM 还在积极探索以下语言的适配：

## 实验性语言列表

- **Rusty Elixir**: 专注于函数式编程与 Actor 模型。
- **Rusty Dart**: 关注 UI 描述能力与强类型脚本语义。
- **Rusty Julia**: 探索科学计算中的多重派发（Multiple Dispatch）。
- **Rusty Cobol**: 展示 Nyar 对古老商业逻辑的兼容能力。
- **Rusty Tcl**: 极简的基于字符串的命令语言。
- **Rusty Scheme/Prolog**: 探索 Lisp 系与逻辑编程范式的映射。
- **Rusty Nim/Nix**: 现代配置与系统构建语言。
- **Rusty Mojo**: 结合 Python 语法与系统级性能。

## 实现状态

目前这些语言处于**骨架（Skeleton）**阶段，已完成基础的项目结构配置与解析器集成，正在进行 `IKunTree` 的 Lowering 逻辑编写。

## 贡献指南

如果您对某个特定语言的运行时实现感兴趣，欢迎参考 [接入新语言指南](index.md#接入新语言指南) 进行贡献。
