//! 过渡层：仍在等待 Oak / Acorn 替代的 `vcc-data` 入口。
//!
//! 新代码不得在此扩展排版或语义规则。只做 re-export 与桥接，便于逐模块删除 `vcc-data`。
//!
//! | 模块 | 待迁入 |
//! |------|--------|
//! | `cst` | `von` / `awsl` 已迁入；`valkyrie` 仍经 `vcc-data`，待 Oak formatter |
//! | `guest_scripts` | 已全部自 vcc-data 迁入，待 `oak-bash` 等正式前端 |
//! | `msil` | `acorn-pe::msil` 已迁入，待 `oak-msil` 正式命名 |
//! | `notedown` | 已自 vcc-data 迁入 `transitional::notedown`，待 `oak-notedown` |
//! | `tgrammar` | 已自 vcc-data 迁入 `transitional::tgrammar`，待 Oak 模板前端 |
pub mod cst;
pub mod guest_scripts;
pub mod msil;
pub mod notedown;
pub mod tgrammar;
