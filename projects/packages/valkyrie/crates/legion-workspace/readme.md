# legion-workspace

Legion 工作区装配层：manifest、规划器、脚本与源码快照。

## 职责

- `legion.von` / `legions.von` 解析与依赖图
- 构建计划、target 选择与源码闭包收集
- 为 `legion` CLI 与 `vcc-napi` 宿主提供共享工作区逻辑

本 crate 不含 CLI 入口与 Nyar 后端降低；编译语义在 `nyar-vm.rs` git 依赖中。

## 构建

```bash
cargo test -p legion-workspace
```
