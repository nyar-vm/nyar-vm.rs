# Rusty Zig

Rusty Zig 展示了如何将具有强静态检查和手动内存管理倾向的语言映射到 Nyar VM。

## 核心特性

- **Comptime**: 利用 Nyar VM 的 JIT 能力实现编译时代码执行。
- **作用域清理 (defer/errdefer)**: 映射到 VM 的栈展开与 Handler 机制，确保资源安全释放。
- **无隐式分配**: 映射 Zig 的内存分配器哲学到 VM 的堆管理。
- **错误处理**: 基于值的错误处理机制，映射到 VM 的 `Value` 标记。

## 实现进度

- [x] 语法解析
- [ ] Comptime 语义支持 (VM 自托管测试)
- [ ] defer/errdefer 逻辑降级 (栈展开测试)
- [ ] 错误集（Error Sets）映射
