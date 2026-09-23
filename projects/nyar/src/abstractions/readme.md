# abstractions

这里放目标无关的后端抽象。

## 职责
- 描述 `TargetFamily`、`ArtifactFormat`、`BackendInputKind` 等共识类型。
- 为各目标 lane 提供统一编排协议，而不是统一物理 `IR`。

## 设计约束
- `BackendInputKind` 只声明“后端真正吃什么”，不暗示这些输入必须共享同一个底层结构。
- 这里允许统一的是 lane 选择、产物描述和接口边界，不允许统一 `CLR / JVM / WASM / GPU` 的物理表示。
- 若新增目标，只应增加新的路线声明与输入种类，而不是给旧输入继续塞兼容字段。

## 接口边界
- 未来这里承接的应是类似 `validate(TInput)` 与 `compile(TInput)` 的最小协议，而不是新的统一 backend object。
- `validate()` 用来拒绝错误路线、未闭合 witness/effect、未满足目标约束的输入。
- `compile()` 只消费通过验证的输入，只做目标相关 lowering、编码、布局和封装。
