# Rusty Ruby

Rusty Ruby 展示了 Nyar VM 对极度动态的对象模型和闭包语义的处理能力。

## 核心特性

- **万物皆对象**: 所有的字面量在 VM 中都被视为 `Object` 或 `DynObject`。
- **动态方法查找**: 利用 VM 的 `DynObject` 和元编程支持，实现 Ruby 风格的方法分发。
- **代码块 (Blocks)**: 映射到 Nyar VM 的 `Closure`（闭包）类型。

## 实现进度

- [x] 语法解析
- [x] 基础对象模型
- [x] 闭包支持
- [ ] 元编程 (method_missing 等) 支持
- [ ] 完整的 Ruby 核心类库映射
