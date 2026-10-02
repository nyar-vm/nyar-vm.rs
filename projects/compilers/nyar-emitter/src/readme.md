# emitter

`emitter` 是 bundled backend 的统一驱动门面。

## 职责
- 接收上层已经完成规划的 `PartitionBackendRequirement` 与目标专用输入。
- 按需求匹配到对应的 bundled compiler。
- 汇总 `ArtifactSet`、入口点与运行契约，返回给 `legion` 这类编排层。

## 分层原则
- `src/lib.rs` 只保留稳定公开接口与共享请求/响应模型。
- `src/driver/*` 负责后端路由、分区编排与 family compiler 注册。
- `src/driver/families/*` 负责单个后端家族的编译细节，不把不同家族的实现混在同一个文件。
- `src/backend/*` 只保留目标后端本体，不再混入 driver 选择逻辑。
- `src/artifacts/*` 收口 sidecar 与产物辅助逻辑。
- 新后端家族接入时，优先新增独立 family compiler，并显式声明自己接受的后端需求。
- driver 自己不再维护第二套选择算法；family compiler 会先注册成 `BackendCandidate`，再交给 `nyar::BackendSelector` 统一选择。

## 当前布局
- `driver/families/clr.rs`：`CLR` 的 bundled 编译链适配。
- `driver/families/jvm.rs`：`JVM` 产物生成与运行契约。
- `driver/families/wasm.rs`：`WASM/WASI` 的产物生成与运行契约。
- `driver/families/native.rs`：`native` 的对象文件输出。
- `driver/families/nyar_vm.rs`：`nyar-vm` 的 sidecar 与 `.nyar` 产物输出。
- `driver/families/mod.rs`：按后端需求注册与查找 family compiler。
- `driver/partitioning.rs`：分区到后端 family 的映射与报告合并。
- `artifacts/suspend_sidecar.rs`：suspend sidecar 序列化与落盘辅助。

## 唯一编译流整改状态

当前生产路线尚未满足唯一事实所有者合同。`BackendPrivatePlan` 由
`CompiledProgram` 生成，但外层仍携带 `AssembledFragment` 与
`FragmentSubmission` 的调用、布局、导入和 suspend 摘要。后端仍消费这些
摘要，不能把私有计划类型已经接入等同于整改完成。

- `ArtifactPartitionPlan` 的产物分区与调度职责不是 `RepresentationPlan`
  的值载体职责；禁止仅按两个名称都有 plan 判定其必须合并。需要清除的是
  调度载荷中与 canonical 事实重复、并且被 lowering 当作语义权威的字段。
- `backend_private_plan.rs` 当前检查表示行是否存在，但尚未把全部表示选择
  编入目标私有计划；同时把 callable 实例重新投影为符号操作数。后续改造
  必须让调用身份和表示选择贯穿准备与编码，不能新增按名字查询的兼容入口。
- Wasm 活跃实现由 `lowering/backends/wasm/mir/mod.rs` 及其显式声明的子模块
  构成。未接入模块树的 `module_build.rs`、`module_imports.rs`、
  `operand_emit.rs`、`emit_instr.rs` 与 `plan.rs` 旧副本已删除；这只是清除
  不可达实现，既不证明活跃 lowering 无旁路，也不证明 compiler capability
  或自举通过。
- 验证生产路线必须从当前源码驱动 Compiler；手工填充旧片段或目标输入的
  测试不能作为语义事实贯通的证据。编码器与执行协议测试可以验证自己的
  低层合同，但不得用来替代全编译流验收。
