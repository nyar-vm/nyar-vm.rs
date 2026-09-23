#![warn(missing_docs)]
#![feature(new_range_api)]
//! Clojure 语言前端

pub mod codegen;
pub mod errors;
pub mod row_type;
pub mod tagless;
pub mod visitor;
pub mod runtime;

pub use crate::runtime::RustyClojureRuntime;

use nyar_types::{Id, NyarContext, NyarError, NyarFrontend, Vfs};
use oak_core::source::SourceText;
use oak_clojure::{ClojureLanguage, ClojureParser};
use chomsky_uir::ConstraintAnalysis;

/// Rusty Clojure 前端
pub struct RustyClojureFrontend {
    language: ClojureLanguage,
}

impl Default for RustyClojureFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl RustyClojureFrontend {
    /// 创建新的前端实例
    pub fn new() -> Self {
        let language = ClojureLanguage::default();
        Self { language }
    }
}

impl NyarFrontend<ConstraintAnalysis, ()> for RustyClojureFrontend {
    type Language = ClojureLanguage;

    fn parse(&self, source: &str) -> Result<(), NyarError> {
        use oak_core::parser::Parser;
        let parser = ClojureParser::new(&self.language);
        let source_text = SourceText::new(source);
        let mut session = oak_core::parser::ParseSession::<ClojureLanguage>::default();
        let output = parser.parse(&source_text, &[], &mut session);
        if output.result.is_ok() {
            println!("Parsed Clojure source");
        }
        output
            .result
            .map(|_| ())
            .map_err(|e| NyarError::Compile(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, _ast: &(), _ctx: &mut NyarContext<'_, V, ConstraintAnalysis>) -> Id {
        let _translator = codegen::NyarTranslator::new();
        // translator.translate_to_id(ast, &mut ctx.builder()).unwrap_or_else(|_| ctx.egraph.add(chomsky_uir::IKun::Seq(vec![])))
        0
    }
}
