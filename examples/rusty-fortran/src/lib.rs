#![warn(missing_docs)]
//! Rusty Fortran 语言前端

pub mod codegen;

use nyar_types::{Id, NyarContext, NyarError, NyarFrontend, Vfs};
use oak_core::source::SourceText;
use oak_fortran::{FortranBuilder, FortranLanguage, ast::FortranRoot};

/// Rusty Fortran 前端
#[derive(Default)]
pub struct RustyFortranFrontend {
    language: FortranLanguage,
}

impl RustyFortranFrontend {
    /// 创建新的前端实例
    pub fn new() -> Self {
        Self {
            language: FortranLanguage::new(),
        }
    }
}

impl NyarFrontend<(), FortranRoot> for RustyFortranFrontend {
    type Language = FortranLanguage;

    fn parse(&self, source: &str) -> Result<FortranRoot, NyarError> {
        let mut session = oak_core::parser::ParseSession::<FortranLanguage>::default();
        let source_text = SourceText::new(source);
        let builder = FortranBuilder::new(&self.language);
        let output = builder.build(&source_text, &[], &mut session);
        output
            .result
            .map_err(|e| NyarError::Compile(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, _ast: &FortranRoot, _ctx: &mut NyarContext<'_, V>) -> Id {
        // let translator = crate::codegen::NyarTranslator::new();
        // let mut translator_ctx = crate::codegen::TranslatorContext::new_with_builder(ctx.builder());
        // translator.translate_root(ast, &mut translator_ctx).unwrap_or_else(|_| ctx.egraph.add(chomsky_uir::IKun::Seq(vec![])))
        0
    }
}
