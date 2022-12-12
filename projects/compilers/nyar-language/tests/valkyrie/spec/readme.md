# compiler tests spec

这里放语义规范测试。

## 目标

- 把 `row / trait / class / sealed class / unite / effect` 的边界先写成可执行护栏。
- 即便当前实现还不完整，也先把场景、命名和预期结果立起来。
- 防止 `HIR`、`MIR`、`nyar` 或某条后端路线把语义悄悄改掉。

## 分组

- `row.rs`: 匿名 `trait` 只做方法行判定，不支持 associated type。
- `trait_system.rs`: 具名 trait 满足必须落到具名 witness。
- `nominal.rs`: `class / sealed class` 只走名义子类型，`unite` 只认已声明 variant 集合。
- `overload.rs`: 固定重载优先级与二义性规则。
- `associated_types.rs`: 关联类型只属于具名 trait，且必须唯一可解。
- `diagnostics.rs`: 报错必须区分 nominal、row、trait、effect。
- `backend_boundary.rs`: 后端不得重新做 row/nominal 判定。

## 策略

- 优先写规范测试，再补实现测试。
- 可以先用 `#[ignore = "..."]` 保留尚未补齐的场景。
- 不允许因为“现在过不了”就把关键语义点留成口头约定。
