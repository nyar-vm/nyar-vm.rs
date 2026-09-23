#![warn(missing_docs)]
#![feature(new_range_api)]
//! Rusty CSharp 语言前端

use nyar_types::{Id, NyarContext, NyarError, NyarFrontend, Vfs};
use oak_core::source::SourceText;
use oak_csharp::{ast::CSharpRoot, CSharpBuilder, CSharpLanguage};

pub mod codegen;
pub mod errors;
pub mod row_type;
pub mod tagless;
pub mod visitor;
pub mod runtime;

pub use crate::runtime::RustyCSharpRuntime;

/// Rusty CSharp 前端
#[derive(Default)]
pub struct RustyCSharpFrontend {
    language: CSharpLanguage,
}

impl RustyCSharpFrontend {
    /// 创建新的前端实例
    pub fn new() -> Self {
        Self {
            language: CSharpLanguage::new(),
        }
    }
}

impl NyarFrontend<(), CSharpRoot> for RustyCSharpFrontend {
    type Language = CSharpLanguage;

    /// 解析 CSharp 源代码
    fn parse(&self, source: &str) -> Result<CSharpRoot, NyarError> {
        let mut session = oak_core::parser::ParseSession::<CSharpLanguage>::default();
        let source_text = SourceText::new(source);
        let builder = CSharpBuilder::new(&self.language);
        let output = builder.build(&source_text, &[], &mut session);
        output
            .result
            .map_err(|e| NyarError::Compile(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(
        &self,
        _ast: &CSharpRoot,
        _ctx: &mut NyarContext<'_, V, ()>,
    ) -> Id {
        // let translator = crate::codegen::NyarTranslator::new();
        // let mut translator_ctx = crate::codegen::TranslatorContext::new_with_builder(ctx.builder());
        // translator.translate_root(ast, &mut translator_ctx).unwrap_or_else(|_| ctx.egraph.add(chomsky_uir::IKun::Seq(vec![])))
        0
    }
}
