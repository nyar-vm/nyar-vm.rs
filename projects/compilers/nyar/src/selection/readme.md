# selection

这里放 backend selector。

## 职责
- 根据 `planning` 产出的 `PartitionBackendRequirement` 和优先级选择 backend。
- 只做选择，不做编译主链的语义判断。

## 设计约束
- 不能因为某个 backend 暂时更“能跑”就把错误路线输入送进去。
- 选择器只消费已经完成规划的后端需求对象，不自行回看优化层状态。
- 选择器不替代 `validate()`；真正的路线边界检查仍在 backend 内完成。
