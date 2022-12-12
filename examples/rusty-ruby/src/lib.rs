//! Rusty Ruby 语言前端
//!
//! 这个库提供了 Rusty Ruby 语言的词法分析、语法分析和 Gaia 翻译功能。

#![warn(missing_docs)]

pub mod codegen;

use nyar_types::{Id, Loc, NyarContext, NyarError, NyarFrontend, Vfs};
use oak_core::{source::SourceText, Builder};
use oak_ruby::{ast::RubyAst, RubyBuilder, RubyLanguage};
use chomsky_uir::ConstraintAnalysis;

/// Rusty Ruby 前端
pub struct RustyRubyFrontend {
    language: RubyLanguage,
}

impl Default for RustyRubyFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl RustyRubyFrontend {
    /// 创建新的前端实例
    pub fn new() -> Self {
        Self {
            language: RubyLanguage::new(),
        }
    }
}

impl NyarFrontend<ConstraintAnalysis, RubyAst> for RustyRubyFrontend {
    type Language = RubyLanguage;

    fn parse(&self, source: &str) -> Result<RubyAst, NyarError> {
        let builder = RubyBuilder::new(&self.language);
        let source_text = SourceText::new(source);
        let mut session = oak_core::parser::ParseSession::<RubyLanguage>::default();

        let output = builder.build(&source_text, &[], &mut session);
        output
            .result
            .map_err(|e| NyarError::Parse(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, _ast: &RubyAst, _ctx: &mut NyarContext<'_, V, ConstraintAnalysis>) -> Id {
        // ctx.egraph.add(chomsky_uir::IKun::Seq(vec![]))
        0
    }
}
