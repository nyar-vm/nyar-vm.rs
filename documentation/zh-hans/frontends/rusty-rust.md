# Rusty Rust

Rusty Rust 展示了如何在 Nyar VM 中实现现代系统级语言的核心语义，特别是其独特的特征系统（Trait System）和所有权模型。

## 核心特性

- **特征系统**: 原生支持 Trait 定义与实现，利用 Nyar VM 的 `TraitObject` 和 `WitnessTable` 实现高效的动态分发。
- **模式匹配 (match + enum)**: 支持复杂的代数数据类型匹配，降级为 VM 的 O(1) 跳转指令。
- **零成本抽象**: 泛型代码通过单态化（Monomorphization）映射到 VM 指令。
- **安全性**: 模拟所有权（Ownership）与生命周期（Lifetime）检查的基础语义。

## 编译与 Lowering 流程

1.  **AST 构建**: 使用 `oak-rust` 解析器生成高度类型化的 AST。
2.  **语义转换**: 通过 `converter` 模块将 Rust AST 转换为 UIR。
3.  **特征见证生成**: 编译器会自动为每个 `impl` 块生成 `WitnessTable`，并在 `TraitObject` 构造时进行绑定。
4.  **匹配优化**: 将 `match` 表达式降级为 `Switch` 或基于 `ValueTag` 的查找表。

## 对象模型处理

Rust 的内存模型在 Nyar VM 中通过以下方式对齐：

- **特征对象**: 映射到 `ValueTag::TraitObject`。它包含数据指针和见证表指针，这与 Rust 的“胖指针”机制完全一致。
- **结构体与枚举**: 
    - `struct` 映射到 `ValueTag::Object`。
    - `enum`（带数据的枚举）映射到带标记的 `Object` 或 `Tuple`。

## 实现进度

- [x] 基础语法解析 (Parser)
- [x] 函数与变量绑定
- [x] 基础特征系统 (Trait System)
- [ ] 模式匹配优化 (O(1) 跳转测试)
- [ ] 生命周期检查 (JIT 运行时验证)
- [ ] 完整标准库映射
