# valkyrie-compiler

`valkyrie-compiler` 负责 Valkyrie 的语言主链与目标前编译阶段。

## 职责
- 承接 parser 输出并完成 `HIR` 建模。
- 维护 `MIR (SSA)`、优化、artifact 分区与目标 `LIR`。
- 保持语义在 `HIR / MIR` 闭合，再把结果分发给具体目标。
- 为 `CLR` 自举主线提供稳定的编译核心。

## 表示边界
- `HIR` 入口收口在 `src/hir`，当前物理承载类型仍复用 `valkyrie-types`，但语义边界属于编译器主链。
- `MIR` 入口收口在 `src/mir`，主表示固定为 `SSA`，必须显式保留基本块、块参数、操作数和终结符。
- `MIR` 中的调用必须区分 `static`、`witness`、`effect-handler` 三类分发，不能把开放语义伪装成普通静态调用。
- `LIR` 入口收口在 `src/lir`，它是目标 lane 感知的低层入口，不再镜像旧版伪 `MIR`。
- 对外公开主链已经切到 `compile_source_to_build_output()` 与 `FrontendNeutralPlan`，legacy `LIR` 已降为 crate 内部显式 lane 的过渡校验资产。

## 当前主线
- `AST -> HIR -> MIR (SSA)` 与 `neutral plan / artifact planning` 已经拆成独立入口，不再要求前端公开暴露一条 `LIR` 主链。
- legacy `LIR` 当前只作为 crate 内部过渡校验边界，相关 lowering、lane 与 pipeline 校验都不再属于公开 API。
- 前端不再默认携带 `CLR` 自举语义，具体后端路线由 `ArtifactPartitionPlan + BackendRegistry` 装配。

## 禁止
- 不回退到统一 `god ir`。
- 不把目标语义补丁藏进 emit 阶段。
- 不把运行时宿主逻辑塞进这里。
- 不把 parser 的词法和语法细节重新复制一份。
