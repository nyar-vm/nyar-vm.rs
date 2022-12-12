//! Rusty Zig 语言前端
//!
//! 这个库提供了 Rusty Zig 语言的词法分析、语法分析和 Gaia 翻译功能。

#![warn(missing_docs)]

pub mod codegen;

use nyar_types::{Id, Loc, NyarContext, NyarError, NyarFrontend, Vfs};
use oak_zig::ZigLanguage;
use chomsky_uir::ConstraintAnalysis;

/// Rusty Zig 前端
pub struct RustyZigFrontend {
    language: ZigLanguage,
}

impl Default for RustyZigFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl RustyZigFrontend {
    /// 创建新的前端实例
    pub fn new() -> Self {
        Self {
            language: ZigLanguage::default(),
        }
    }
}

impl NyarFrontend<ConstraintAnalysis, ()> for RustyZigFrontend {
    type Language = ZigLanguage;

    fn parse(&self, _source: &str) -> Result<(), NyarError> {
        // TODO: 实现真正的解析逻辑
        Ok(())
    }

    fn lower_unified<V: Vfs>(&self, _ast: &(), _ctx: &mut NyarContext<'_, V, ConstraintAnalysis>) -> Id {
        // TODO: 实现真正的从 AST 到 IKunTree 的转换
        // ctx.builder().seq(vec![], Default::default())
        0
    }
}
