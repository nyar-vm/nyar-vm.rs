# Rusty Go

Rusty Go 专注于展示 Nyar VM 对高并发语义和结构化类型系统（Structural Typing）的支持。

## 核心特性

- **并发模型**: `goroutine` 映射到 Nyar VM 的绿色线程或 `Continuation`。
- **Channel**: 原生同步原语，基于 VM 的原子操作实现。
- **错误处理 (defer/panic/recover)**: 映射到 VM 的 `ExceptionHandler` 栈与截断机制。
- **结构化接口**: 利用 `WitnessTable` 实现非侵入式的接口绑定。

## 编译与 Lowering 流程

1.  **解析**: 使用 `oak-go` 获取源代码结构。
2.  **接口绑定**: 在运行时动态计算接口满足性，并生成相应的见证表。
3.  **并发降级**: `go` 关键字被降级为 VM 的协程创建指令。

## 对象模型处理

- **结构体**: 映射到 `ValueTag::Object`。
- **接口**: 类似于 Rust 的特征对象，但在进入 VM 前由编译器或运行时进行隐式转换。

## 实现进度

- [x] 基础语法解析
- [ ] 协程与 Channel 支持
- [ ] defer/panic/recover 语义 (异常截断测试)
- [ ] 接口（Interface）动态绑定
