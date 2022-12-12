# interpreter src

这里是运行时调度源码。

## 职责
- 组织各目标 runtime descriptor。
- 提供 `effects`、`ffi`、registry 与运行时分发。
- 为 `legion run` 维护宿主侧目标映射。
- 让各目标运行时各管各的，不共享混乱宿主逻辑。

## 目标映射
- `jvm` -> `java`
- `clr` -> `dotnet`
- `wasm` -> `node`
- `windows` -> `exe`
- `wasi` -> `wasmtime`

## 禁止
- 不在这里替编译器补 lowering。
- 不让单个目标目录长成跨平台总入口。
