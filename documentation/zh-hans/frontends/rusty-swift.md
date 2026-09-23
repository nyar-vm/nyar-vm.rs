# Rusty Swift

Rusty Swift 展示了如何结合引用计数语义（ARC）与强类型协议系统。

## 核心特性

- **协议 (Protocols)**: 映射到 Nyar VM 的 `TraitObject`。
- **选型 (Optionals)**: 利用 VM 的 `Null` 和带标签的 `Value` 实现。
- **安全性**: 强制的初始化检查与边界检查。

## 实现进度

- [x] 语法解析
- [x] 类与结构体定义
- [ ] 完整的协议分发
- [ ] 自动引用计数 (ARC) 与 Nyar GC 的集成
