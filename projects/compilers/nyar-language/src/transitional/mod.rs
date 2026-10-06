//! 过渡层：自 `vcc-data` 内联的 lexer / CST / guest 脚本，等待 Oak / Acorn 正式 crate 接管。
//!
//! **格式化不在此扩展**：合法 **SourceFormatter** 只能落在上游 `oak-<language>/src/formatter/`
//!（对齐 `oak-typescript`）。`nyar-analyzer::format` 仅是注册表与 `Document` 布局契约。
//! **Printer**（AST / 值模型 → 文本）与 formatter 严格分离，见 `valkyrie::printer`。
//!
//! | 模块 | 状态 |
//! |------|------|
//! | `cst` | legacy Valkyrie/VON CST 仅 `#[cfg(test)]`；AWSL 生产解析在 `oak-awsl` |
//! | `guest_scripts` | 内联 guest parser，待 `oak-bash` 等正式前端 |
//! | `msil` | `acorn-pe::msil`，待 `oak-msil` |
//! | `notedown` | 内联，待 `oak-notedown` |
//! | `tgrammar` | 内联，待 Oak 模板前端 |
pub mod cst;
pub mod guest_scripts;
pub mod msil;
pub mod notedown;
pub mod tgrammar;
