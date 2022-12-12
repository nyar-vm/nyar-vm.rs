# compiler tests pipeline

这里验证 parser 输出进入 `HIR / MIR / LIR` 的链路，不允许重新发明统一伪 `IR`。

## 重点

- 保证 `row` 在进入 `MIR` 前已经闭合为成员调用，而不是开放 evidence。
- 保证 `trait / effect` 的开放调度不会被过早伪装成静态调用。
- 保证 `ArtifactPartitionPlan` 之后进入的是 target-specific input，而不是跨端统一壳。

