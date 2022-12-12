# backends

这里放目标相关后端实现与最小 backend 接口。

## 职责
- 每个 backend 显式声明自己消费的输入类型。
- 每个 backend 分开实现 `validate()` 与 `compile()`。
- `validate()` 负责边界检查，不负责补语言语义。
- `compile()` 只负责目标相关编码、布局、封装与产物生成。

## 详细约束
- `CLR` backend 只接受合法的 `ClrImage` 或后续等价输入。
- `JVM` backend 只接受 `JvmClassFile` 风格输入。
- `WASM` backend 只接受 `WasmModule` 风格输入。
- `native` backend 只接受 native lane 的低层输入，不得直接吃 `CLR/JVM/WASM` 容器模型。
- 若输入仍残留未闭合 witness、未静态化 effect 调度、未决 row/nominal 判定，必须在 `validate()` 失败。

## 与 HIR / MIR / LIR 的关系
- backend 不拥有 `HIR`。
- backend 不拥有语言级 `MIR`。
- backend 只消费目标路线自己的 `LIR / Backend Input`。
