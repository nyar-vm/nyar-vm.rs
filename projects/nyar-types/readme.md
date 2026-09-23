# nyar-types

`nyar-types` 是 `nyar` 平台的底层共享类型包。

## 职责
- 提供跨前端、跨分析层、跨 backend 都稳定成立的基础类型。
- 提供通用名类型、能力标签、逻辑符号标识、最小错误载体。
- 提供最小外部导入链接描述，例如 `ExternalImportLink` 这类跨层稳定契约。
- 提供 backend-private `executable` / `layout` 契约（lowering 视图，不是语言主链 IR）。
- 不承载语言级 `HIR / MIR / LIR`，也不承载目标专属容器模型。

## 禁止
- 不在这里扩张成新的 `god object` 类型仓库。
- 不把后端容器、语言语义和运行时状态全塞进来。
- 不为了兼容旧遗产而引入大而全历史模型。
- 不把 executable 视图宣传成跨语言 god IR / 语义总线。
