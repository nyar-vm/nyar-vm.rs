# lanes

这里放 target lane。

## 职责
- 承接 `ArtifactPartitionPlan` 之后的每个分区。
- 把分区结果降到本路线真正消费的 backend input。
- 维护“每条路线只对自己负责”的边界。

## 详细设计
- `CPU / VM` lane 可把分区降到 `NyarIR` 或等价低层表示。
- `CLR` lane 应把分区降到 `ClrImage`、元数据、`MSIL`、`PE` 等低层输入。
- `JVM` lane 应把分区降到 `ClassFile` 风格输入。
- `WASM` lane 应把分区降到结构化控制流与 section 模型。
- `GPU / Shader` lane 应直接降到目标专用模型，不借道 CPU 导向兼容壳。

## 禁止
- 不做 trait resolve。
- 不做 row 成员选择。
- 不做 nominal subtype 判定。
- 不做 effect handler 选择。
- 不把开放 witness 伪装成普通静态调用继续下沉。
