//! 过渡层：仍在等待 Oak 替代的 `vcc-data` 入口。
//!
//! 新代码不得在此扩展格式规则。只做 re-export 与桥接，便于逐后端迁入 `acorn-*` / `oak-*`。
//!
//! | 模块 | 状态 |
//! |------|------|
//! | `binary` | `acorn-jvm` + `acorn-pe` 已覆盖；`nyar-emitter` 不再直接依赖 `vcc-data` |
//! | `msil` | 经 `acorn-pe::msil` 过渡，待 `oak-msil` |

pub mod binary;
pub mod msil;
