//! 过渡层：notedown / tgrammar，等待独立 crate 或 `oak-notedown` 接管。
//!
//! **文本解析/格式化不在此扩展**：Valkyrie / VON / AWSL 生产路径在 `oak-valkyrie` / `oak-von` / `oak-awsl`。
//! 合法 **SourceFormatter** 只能落在上游 `oak-<language>/src/formatter/`（对齐 `oak-typescript`）。
//!
//! | 模块 | 状态 |
//! |------|------|
//! | `notedown` | 内联，待 `oak-notedown` |
//! | `tgrammar` | 仅迁移对照；生产 TGrammar 由 `oak-valkyrie` + `support_t_grammar` 提供 AST 节点 |
pub mod notedown;
pub mod tgrammar;
