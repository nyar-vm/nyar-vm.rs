# 编译后端支持

本目录描述 Nyar 项目的后端，其中 Nyar VM 是一个独立后端；`nyar-ir`、Nyar 字节码和 VM 运行时只属于该后端。JVM、CLR、Wasm、Native 与 Nyar VM 并列，各自维护 lowering、IR 和执行合同。

## 后端列表

- [Nyar VM](nyar-vm.md): 原生字节码后端，支持 JIT 与解释执行。
- [WebAssembly (WASM)](wasm.md): 针对 Web 与云计算环境的 WASI 兼容后端。
- [Native](native.md): 针对 x86_64, ARM64 等架构的原生机器码后端。
- [JVM](jvm.md): 针对 Java 虚拟机的后端，输出 `.class` 或 `.jar` 文件。
- [CLR](clr.md): 针对 .NET 公共语言运行时的后端。

## 统一发射接口

每个 backend 消费自己声明的 lowering 输入 contract，明确能力边界和失败方式，再转换为自己的目标指令集。选择 Nyar VM 的前端才生成 `nyar-ir`/Nyar 字节码；其他 backend 不经过 `nyar-ir`，也不把它当作共同 IR。
