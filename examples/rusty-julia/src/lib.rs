#![warn(missing_docs)]
//! Rusty Julia 语言前端

pub mod codegen;

use nyar_types::{Id, NyarContext, NyarError, NyarFrontend, Vfs};
use oak_core::source::SourceText;
use oak_julia::{JuliaBuilder, JuliaLanguage, JuliaRoot};
use chomsky_uir::ConstraintAnalysis;

/// Rusty Julia 前端
pub struct RustyJuliaFrontend {
    language: JuliaLanguage,
}

impl Default for RustyJuliaFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl RustyJuliaFrontend {
    /// 创建新的前端实例
    pub fn new() -> Self {
        Self {
            language: JuliaLanguage::default(),
        }
    }
}

impl NyarFrontend<ConstraintAnalysis, JuliaRoot> for RustyJuliaFrontend {
    type Language = JuliaLanguage;

    fn parse(&self, source: &str) -> Result<JuliaRoot, NyarError> {
        let builder = JuliaBuilder::new(&self.language);
        let source_text = SourceText::new(source);
        let mut session = oak_core::parser::ParseSession::<JuliaLanguage>::default();

        let output = builder.build(&source_text, &[], &mut session);
        output
            .result
            .map_err(|e| NyarError::Parse(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, _ast: &JuliaRoot, _ctx: &mut NyarContext<'_, V, ConstraintAnalysis>) -> Id {
        // let mut converter = crate::codegen::UirConverter::new(ctx);
        // converter.convert_root(ast)
        0
    }
}
