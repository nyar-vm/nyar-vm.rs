---
layout: home

hero:
  name: "Nyar"
  text: "现代通用虚拟机运行时"
  tagline: 融合 Chomsky 普遍优化与栈式机架构的下一代执行环境
  image:
    src: /logo.svg
    alt: Nyar
  actions:
    - theme: brand
      text: 快速开始
      link: /guide/getting-started
    - theme: alt
      text: 查看示例
      link: /examples/

features:
  - icon: 🧠
    title: Chomsky 普遍优化
    details: 基于 E-Graph 饱和搜索与成本模型提取，彻底解决相位顺序问题，自动发现程序的最优执行路径。
  - icon: 🥞
    title: 栈式机架构 (LIR)
    details: 采用精简的栈式指令集，指令格式紧凑，解释执行高效，且易于映射到 WASM、JVM 等主流运行时。
  - icon: 🚀
    title: 多目标 AOT 编译
    details: 通过 nyar-aot 将程序静态编译到 WebAssembly、原生机器码或托管平台，真正实现“一次编写，到处优化”。
  - icon: 🎭
    title: 代数效应支持
    details: 原生支持代数效应，在虚拟机层面提供高效的控制流跳转，优雅处理异常、异步与状态管理。
  - icon: 🔒
    title: 强类型安全
    details: 从 HIR 到 SSA 全程保留类型约束，配合约束感知合并策略，确保优化过程中的语义等价性与安全性。
  - icon: 🛠️
    title: 模块化工具链
    details: 包含 nyar-tools, nyar-vm, nyar-aot 等核心组件，支持增量编译、即时调试与深度性能分析。
---

## 什么是 Nyar？

Nyar 是一个现代的通用虚拟机运行时，专为构建可靠、高性能的分布式应用与跨平台工具而设计。它将 Chomsky 普遍优化引擎与高效的栈式机架构相结合，为下一代编程语言提供坚实的执行基础。

### 核心特性

- **普遍优化**: 利用 E-Graph 饱和搜索自动发现最优执行路径
- **栈式架构**: 紧凑的 LIR 指令集，兼顾解释效率与跨平台对齐
- **多目标编译**: 支持 AOT 编译至 WebAssembly、原生机器码及托管运行时
- **代数效应**: 虚拟机层面的控制流抽象，统一处理副作用
- **模块化设计**: nyar-vm, nyar-aot, nyar-gc 等组件深度解耦
- **开发者体验**: 高质量诊断信息，支持热重载与增量编译

### 快速示例

```nyar
// Nyar LIR 示例 (伪代码)
.function main(argc: i32, argv: ptr) -> i32 {
    // 压入操作数
    push 10
    push 20
    
    // 执行栈式加法
    i32.add
    
    // 调用内置打印
    call ↯print_i32
    
    // 返回结果
    push 0
    return
}
```

这段简单的 Nyar LIR 展示了：
- 基于栈的操作数管理
- 类型化的指令执行 (`i32.add`)
- 显式的函数调用与返回
- 紧凑的指令布局

## 开始使用

准备好体验 Nyar 的强大功能了吗？

[快速开始 →](/guide/getting-started)

## 为什么选择 Nyar？

### 🎯 **解决真实问题**
传统编译器在处理复杂优化时往往陷入“相位顺序问题”。Nyar 通过集成 Chomsky 引擎，将优化转变为对等价空间的科学搜索，让开发者无需手动编排复杂的优化 Pass 也能获得极致性能。

### 🔧 **现代化设计**
Valkyrie 从零开始设计，吸收了函数式编程、类型理论和编程语言设计的最新成果，提供了一种既强大又易用的编程体验。

### 🌍 **广泛适用**
无论是前端应用、后端服务还是系统工具，Valkyrie 都能胜任。多目标编译能力让你的代码可以运行在任何平台上。

### 🚀 **性能优异**
先进的编译器优化技术，包括尾调用优化、内联、死代码消除等，确保生成的代码具有出色的运行时性能。