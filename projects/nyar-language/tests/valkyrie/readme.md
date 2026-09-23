# compiler tests

Valkyrie 编译器集成测试，按编译阶段与语义域分层组织。

## 顶层模块

| 目录 | 职责 |
|------|------|
| `smoke.rs` | 端到端冒烟：parse → HIR → MIR 最小路径 |
| `pipeline/` | `AST → HIR → MIR` 主链与调度器 |
| `control_flow/` | 控制流校验、标签、try/?、fallthrough（`fallthrough.rs` 已就位，待启用）、共享 fixture |
| `mir/` | MIR lowering、pattern dispatch、value layout、state machine |
| `type_checker/` | 类型检查、约束求解、pattern、overload |
| `typing/` | MRO（C3）、继承冲突分析、escape analysis |
| `oop/` | OOP witness、parent slots、匿名 class |
| `spec/` | 语义规范测试（row / trait / class / sealed / effect） |
| `optimizer/` | 静态化、witness 消除、封闭类优化 |
| `derive/` | derive 宏展开 |
| `frontend_contract/` | suspend plan、future/block 协议；`export_planning.rs` 待启用 |
| `module/` | 模块图与解析错误 |
| `highlight/` | 文本快照 highlight 回归（`.highlight` sidecar） |

## 使用约定

- 新增测试放入对应子目录，不要在 `tests/valkyrie/` 根目录堆 `.rs` 文件。
- 跨层控制流断言复用 `control_flow/fixtures.rs` 中的源码常量与 shape helper。
- `spec/` 允许 `#[ignore = "..."]` 保留尚未补齐的语义场景。
- highlight 文本快照：源文件在 `legion/tests/fixtures/text/valkyrie/`，runner 在 `highlight/fixtures.rs`；重生成 `NYAR_TEST_REGENERATE=1 cargo test -p nyar-language text_fixture_highlight_regression`（本 crate 不持有 fixture）。

## CLR 自举契约

与 `valkyrie.v/projects/legion._/projects/legion.tools` 自举验收对齐（同构主线：`language`→`analyzer`→`optimizer`→`emitter`，中性输入为 `ExecutableModule`）：

- `pipeline/`：CLR smoke、`[clr(...)]` micro lowering、多文件 namespace 闭包
- `optimizer/`：MIR witness 静态化前提（开放 witness 不得进入 CLR lane）
- `legion/tests/cmds_build.rs`：`legion.tools` 同构 fixture
- `emitter/tests.rs`：CLR lane 拒绝未静态化 witness / suspend 片段
