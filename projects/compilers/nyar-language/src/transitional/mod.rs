//! 过渡层：自 `vcc-data` 内联的 guest 脚本等，等待 Oak / Acorn 正式 crate 接管。
//!
//! **文本解析/格式化不在此扩展**：Valkyrie / VON / AWSL 生产路径在 `oak-valkyrie` / `oak-von` / `oak-awsl`。
//! 合法 **SourceFormatter** 只能落在上游 `oak-<language>/src/formatter/`（对齐 `oak-typescript`）。
//!
//! | 模块 | 状态 |
//! |------|------|
//! | `guest_scripts` | 内联 guest parser，待 `oak-bash` 等正式前端 |
//! | `msil` | `acorn-pe::msil`，待 `oak-msil` |
//! | `notedown` | 内联，待 `oak-notedown` |
//! | `tgrammar` | 仅迁移对照；生产 TGrammar 由 `oak-valkyrie` + `support_t_grammar` 提供 AST 节点 |
pub mod guest_scripts;
pub mod msil;
pub mod notedown;
pub mod tgrammar;
