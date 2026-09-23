#![warn(missing_docs)]
//! Rusty Dart 语言前端

pub mod codegen;

use nyar_types::{Id, NyarContext, NyarError, NyarFrontend, Vfs};
use oak_core::source::SourceText;
use oak_dart::{DartBuilder, DartLanguage, DartRoot};

/// Rusty Dart 前端
#[derive(Default)]
pub struct RustyDartFrontend;

impl RustyDartFrontend {
    /// 创建新的前端实例
    pub fn new() -> Self {
        Self
    }
}

impl NyarFrontend<(), DartRoot> for RustyDartFrontend {
    type Language = DartLanguage;

    fn parse(&self, source: &str) -> Result<DartRoot, NyarError> {
        let language = DartLanguage::default();
        let builder = DartBuilder::new(&language);
        let source_text = SourceText::new(source);
        let mut session = oak_core::parser::ParseSession::<DartLanguage>::default();

        let output = builder.build(&source_text, &[], &mut session);
        output
            .result
            .map_err(|e| NyarError::Parse(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, _ast: &DartRoot, _ctx: &mut NyarContext<'_, V>) -> Id {
        // let mut converter = codegen::UirConverter::new(ctx);
        // converter.convert_root(ast)
        0
    }
}
