# planning

这里放 `nyar` 自己拥有的中性编排计划。

## 职责
- 只承接已经验证的 `CanonicalProgram`，不接受独立前端事实作为成功入口。
- 组合目标、lane、能力和运行时需求，形成 `ArtifactPartitionPlan`。
- 从分区计划中产出已经收口好的 `PartitionBackendRequirement`，供选择层消费。
- 在分区计划里显式携带宿主边界与引用对象管理策略，避免把 `GC/RC` 混进后端家族或启动器细节。
- 为 backend 选择、后续 lowering 和打包提供稳定入口。
- 分区入口、操作根与片段视图使用 `ItemInstanceId`，直接消费 Canonical 片段合同。
- 等式优化的名称视图不携带入口、导入和调用边，不得通过名称等式重绑定 callable 根。

## 禁止
- 不直接依赖具体前端类型。
- 不回流解释语言级语义。
- 不承担目标容器编码职责。
- 不把等式优化后的名称列表当作函数闭包；装配必须拒绝与 Canonical 不一致的身份。
