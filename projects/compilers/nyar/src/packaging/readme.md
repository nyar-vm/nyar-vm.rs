# packaging

这里放目标 lane 与产物交付协议。

## 职责
- 描述 `OutputSpec`、`ArtifactSet`、`TargetLane`。
- 统一编排协议，不统一物理 `IR`。

## 架构含义
- 这里对应 `ArtifactPartitionPlan -> target lane -> backend input -> artifact set` 之间的承接层。
- `packaging` 只负责“送去哪条路线、打成什么产物”，不负责重新解释语言语义。
- 每条 lane 进入的都应是该目标真正支持的低层输入；不合法输入必须在进入后端前被拒绝。

## 与 Assembler 文档对齐
- 这里对应旧文档里 `Assembler` 保留的那一层：选择、验证、调用后端、交付产物。
- 它不负责 trait resolve，不负责 row 判定，不负责 effect handler 选择，也不负责把开放 witness 改写成静态函数。
- `OutputSpec` 与 `ArtifactSet` 只描述交付结果，不承载语言级真相。
