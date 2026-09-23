# valkyrie-interpreter

`nyar-runner` 为 `legion run` 提供宿主支持，负责目标运行时调度与宿主接口，不承担语言主链设计。

## 职责
- 为 `CLR / JVM / WASI / WASM / Windows` 提供独立运行时入口。
- 维护运行时注册表、FFI、effects 与目标描述。
- 消费编译器产物并调度到对应目标。

## 目标映射
- `src/jvm` 跑 `java`。
- `src/clr` 跑 `dotnet`。
- `src/wasm` 跑 `node`。
- `src/windows` 跑 `exe`。
- `src/wasi` 跑 `wasmtime`。

## 禁止
- 不在这里重新定义 `HIR / MIR / LIR`。
- 不把类型检查和语言语义闭合逻辑塞回运行时。
- 不通过 runtime host bridge 规避 `CLR` 源头自举问题。
