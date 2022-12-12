# 编译后端支持

Nyar VM 通过 `nyar-aot` 和 `ProjectChomsky` 框架支持多种编译目标。

## 后端列表

- [Nyar VM](nyar-vm.md): 原生字节码后端，支持 JIT 与解释执行。
- [WebAssembly (WASM)](wasm.md): 针对 Web 与云计算环境的 WASI 兼容后端。
- [Native](native.md): 针对 x86_64, ARM64 等架构的原生机器码后端。
- [JVM](jvm.md): 针对 Java 虚拟机的后端，输出 `.class` 或 `.jar` 文件。
- [CLR](clr.md): 针对 .NET 公共语言运行时的后端。

## 统一发射接口

所有后端均实现 `chomsky_extract::Backend` trait，确保优化后的 UIR 能够无缝转换为目标指令集。
