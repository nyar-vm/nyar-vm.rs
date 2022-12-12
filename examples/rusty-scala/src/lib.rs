#![warn(missing_docs)]
#![feature(new_range_api)]
//! Mini Scala 语言前端

pub mod codegen;
pub mod errors;
pub mod row_type;
pub mod tagless;
pub mod visitor;

use nyar_types::{NyarContext, NyarError, NyarFrontend, Id, Vfs};
use oak_core::source::SourceText;
use oak_scala::{ScalaBuilder, ScalaLanguage, ScalaRoot};
use chomsky_uir::ConstraintAnalysis;

/// Rusty Scala 前端
pub struct RustyScalaFrontend {
    language: ScalaLanguage,
    builder: ScalaBuilder<'static>,
}

impl Default for RustyScalaFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl RustyScalaFrontend {
    /// 创建新的前端实例
    pub fn new() -> Self {
        let language = Box::leak(Box::new(ScalaLanguage::default()));
        let builder = ScalaBuilder::new(language);
        Self {
            language: language.clone(),
            builder,
        }
    }
}

impl NyarFrontend<ConstraintAnalysis, ScalaRoot> for RustyScalaFrontend {
    type Language = ScalaLanguage;

    fn parse(&self, source: &str) -> Result<ScalaRoot, NyarError> {
        let mut session = oak_core::parser::ParseSession::<ScalaLanguage>::default();
        let source_text = SourceText::new(source);
        let output = self.builder.build(&source_text, &[], &mut session);
        if let Ok(_root) = &output.result {
            println!("Parsed Scala root");
        }
        output
            .result
            .map_err(|e| NyarError::Compile(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, _ast: &ScalaRoot, _ctx: &mut NyarContext<'_, V, ConstraintAnalysis>) -> Id {
        let _translator = codegen::NyarTranslator::new();
        // translator.translate_to_id(ast, &mut ctx.builder()).unwrap_or_else(|_| ctx.egraph.add(chomsky_uir::IKun::Seq(vec![])))
        0
    }
}
