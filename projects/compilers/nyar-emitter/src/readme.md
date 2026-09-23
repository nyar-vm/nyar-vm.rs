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
