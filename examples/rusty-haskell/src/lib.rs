//! Rusty Haskell 语言前端
//!
//! 这个库提供了 Rusty Haskell 语言的词法分析、语法分析和 Nyar 翻译功能。

#![warn(missing_docs)]

pub mod codegen;

use nyar_types::{Id, NyarContext, NyarError, NyarFrontend, Vfs};
use oak_core::{source::SourceText, Builder};
use oak_haskell::{HaskellLanguage};

/// Rusty Haskell 前端
pub struct RustyHaskellFrontend {
    language: HaskellLanguage,
}

impl Default for RustyHaskellFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl RustyHaskellFrontend {
    /// 创建新的前端实例
    pub fn new() -> Self {
        Self {
            language: HaskellLanguage::new(),
        }
    }
}

impl NyarFrontend<(), oak_haskell::ast::HaskellRoot> for RustyHaskellFrontend {
    type Language = HaskellLanguage;

    fn parse(&self, source: &str) -> Result<oak_haskell::ast::HaskellRoot, NyarError> {
        let source_text = SourceText::new(source);
        let mut session = oak_core::parser::ParseSession::<HaskellLanguage>::default();
        let builder = oak_haskell::builder::HaskellBuilder::new(&self.language);
        let output = builder.build(&source_text, &[], &mut session);

        output.result
            .map_err(|e| NyarError::Parse(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(
        &self,
        _ast: &oak_haskell::ast::HaskellRoot,
        _ctx: &mut NyarContext<'_, V>,
    ) -> Id {
        let _translator = crate::codegen::NyarTranslator::new();
        // let mut translator_ctx = crate::codegen::TranslatorContext::new_with_builder(ctx.builder());
        // translator.translate_root(ast, &mut translator_ctx).unwrap_or_else(|_| ctx.egraph.add(chomsky_uir::IKun::Seq(vec![])))
        0
    }
}
