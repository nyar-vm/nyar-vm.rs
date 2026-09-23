# compiler tests

Valkyrie 编译器集成测试，按编译阶段与语义域分层组织。

## 顶层模块

| 目录 | 职责 |
|------|------|
| `smoke.rs` | 端到端冒烟：parse → HIR → MIR 最小路径 |
| `pipeline/` | `AST → HIR → MIR` 主链与调度器 |
| `control_flow/` | 控制流校验、标签、try/?、fallthrough |
| `mir/` | MIR lowering、pattern dispatch、value layout |
| `type_checker/` | 类型检查、约束求解、pattern、overload |
| `typing/` | MRO（C3）、继承冲突分析 |
| `oop/` | OOP witness、parent slots |
| `spec/` | 语义规范测试 |
| `optimizer/` | 静态化、witness 消除 |
| `derive/` | derive 宏展开 |
| `module/` | 模块图与解析错误 |
| `highlight/` | highlight 文本快照回归 |

## 约定

- 新增测试放入对应子目录，不要在 `tests/valkyrie/` 根目录堆 `.rs` 文件。
- `spec/` 允许 `#[ignore]` 保留尚未实现的语义场景。
- highlight 快照重生成：`NYAR_TEST_REGENERATE=1 cargo test -p nyar-language text_fixture_highlight_regression`
