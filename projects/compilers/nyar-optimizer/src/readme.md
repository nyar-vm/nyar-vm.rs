# nyar-optimizer

`nyar-optimizer` 提供 `nyar` 平台的 `Object Algebraic`、`E-Graph` 和 `Futamura projection` 优化骨架。

## 职责
- 承接已经完成语义闭合的 `Object Algebraic` 程序边界。
- 维护等价重写规则、`E-Graph` 会话和提取策略。
- 为不同目标族选择对应的 `futa_*` 投影家族，而不是输出一份闭合的统一 `IR`。

## 当前边界
- 当前实现先固定组合接口、规则理论和投影边界。
- 当前实现明确拒绝把 `Object Algebraic` 简化成单一节点枚举。
- 当前实现把 `Futamura projection` 视为目标家族投影，而不是 emit 前的小别名步骤。

## 禁止
- 不复制 `nyar-analyzer` 的 `ProgramFacts` 事实层结构。
- 不把所有后端重新糊成单一 `god ir`。
- 不把目标特定编码层误叫成统一后端表示。
