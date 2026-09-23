# nyar-tools

`nyar-tools`（二进制名为 `nyar`）是 Nyar VM 的官方命令行工具链，集成了运行、编译、调试和基准测试功能。

## 核心职责

- **Command Parsing**: 优雅地处理子命令与参数。
- **REPL**: 提供交互式执行环境。
- **Development Support**: 包含字节码转储（Dump）与反汇编工具。

## 子命令手册

### `run`
执行一个 `.nyar` 字节码文件或直接运行源码。
```bash
nyar run hello.nyar
nyar run hello.java
```

### `compile`
将源码或字节码编译为指定的后端构件。
```bash
nyar compile main.kt --target wasm --output main.wasm
```

### `dump`
反汇编 `.nyar` 文件，用于检查生成的指令流。
```bash
nyar dump app.nyar
```

### `repl`
启动交互式 Shell，支持快速验证语法和表达式。
```bash
nyar repl
```

### `bench`
对指定的代码块进行基准测试，衡量 JIT 或 AOT 的执行效率。

## 内部实现说明

`nyar-tools` 本身并不包含核心逻辑，它通过调用 `nyar-vm`, `nyar-aot`, `nyar-jit` 等库来完成任务。

- **`cmds/`**: 包含每个子命令的具体 CLI 逻辑。
