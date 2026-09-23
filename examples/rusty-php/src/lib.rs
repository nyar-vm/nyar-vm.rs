//! Rusty PHP 语言前端
//!
//! 这个库提供了 Rusty PHP 语言的词法分析、语法分析和 Gaia 翻译功能。

pub mod codegen;

use nyar_types::{NyarContext, NyarError, NyarFrontend, Id, Vfs};
use oak_core::{source::SourceText, Builder};
use oak_php::{PhpBuilder, PhpLanguage, PhpRoot};
use chomsky_uir::ConstraintAnalysis;

/// Rusty PHP 前端
pub struct RustyPhpFrontend {
    language: PhpLanguage,
}

impl Default for RustyPhpFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl RustyPhpFrontend {
    /// 创建新的前端实例
    pub fn new() -> Self {
        Self {
            language: PhpLanguage::default(),
        }
    }
}

impl NyarFrontend<ConstraintAnalysis, PhpRoot> for RustyPhpFrontend {
    type Language = PhpLanguage;

    fn parse(&self, source: &str) -> Result<PhpRoot, NyarError> {
        let builder = PhpBuilder::new(&self.language);
        let source_text = SourceText::new(source);
        let mut session = oak_core::parser::ParseSession::<PhpLanguage>::default();

        let output = builder.build(&source_text, &[], &mut session);
        output
            .result
            .map_err(|e| NyarError::Parse(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, _ast: &PhpRoot, _ctx: &mut NyarContext<'_, V, ConstraintAnalysis>) -> Id {
        // ctx.egraph.add(chomsky_uir::IKun::Seq(vec![]))
        0
    }
}
