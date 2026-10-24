//! 语言内建符号身份判定。
//!
//! **batch 7 收紧**：`is_language_builtin_symbol` 是迁移期遗留——按符号路径段猜测 intrinsic 身份。
//! 生产 validation 核心逻辑**未使用**此函数；仅 re-export 保留。
//! intrinsic 身份必须由 overload 阶段结构化绑定（`HirResolvedCall.intrinsic`），调用点不得按名字猜。

use crate::types::NamePath;

/// 判断路径是否为语言内建 intrinsic。
///
/// **batch 7**：生产代码已不再调用。仅保留为公开 API（validation.rs 对外 re-export），
/// 实际 lowering 不再依赖名字匹配——intrinsic 必须带结构化 `IntrinsicId` 字段。
#[doc(hidden)]
pub fn is_language_builtin_symbol(symbol: &NamePath) -> bool {
    let head = symbol.parts().first().map(|p| p.as_str()).unwrap_or("");
    head == "builtin"
}
