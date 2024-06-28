# Pending emitter integration tests

这些用例仍引用迁移前的 `FragmentSubmission` / `executable_provider` 合同，暂不参与 `cargo test`。

修复方向：改为通过 `TargetExecutionPlan` + `nyar_emitter::testing::empty_fragment_submission()` 构造载荷，再移回 `tests/` 并在 `nyar-emitter/Cargo.toml` 注册 `[[test]]`。
