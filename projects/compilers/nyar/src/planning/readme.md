# planning

这里放 `nyar` 自己拥有的中性编排计划。

## 职责
- 承接下游前端已经闭合好的 `ProgramFacts`。
- 组合目标、lane、能力和运行时需求，形成 `ArtifactPartitionPlan`。
- 从分区计划中产出已经收口好的 `PartitionBackendRequirement`，供选择层消费。
- 在分区计划里显式携带宿主边界与引用对象管理策略，避免把 `GC/RC` 混进后端家族或启动器细节。
- 为 backend 选择、后续 lowering 和打包提供稳定入口。

## 禁止
- 不直接依赖具体前端类型。
- 不回流解释语言级语义。
- 不承担目标容器编码职责。
