#![warn(missing_docs)]
//! Mini Rust 语言实现
#![feature(new_range_api)]

pub mod ast;
pub mod converter;
pub mod runtime;

pub use crate::runtime::RustyRustRuntime;

use chomsky_uir::{Analysis, IKun, egraph::HasDebugInfo};
use nyar_types::{Id, NyarContext, NyarError, NyarFrontend, Vfs};
use oak_core::source::SourceText;
use oak_rust::{RustBuilder, RustLanguage, RustRoot};

/// Mini Rust 前端实现
#[derive(Default)]
pub struct MiniRustFrontend {
    language: RustLanguage,
}

impl MiniRustFrontend {
    /// 创建一个新的 Mini Rust 前端
    pub fn new() -> Self {
        Self {
            language: RustLanguage::default(),
        }
    }
}

impl<A: Analysis<IKun> + 'static> NyarFrontend<A, RustRoot> for MiniRustFrontend 
where A::Data: HasDebugInfo
{
    type Language = RustLanguage;

    fn parse(&self, source: &str) -> Result<RustRoot, NyarError> {
        let builder = RustBuilder::new(&self.language);
        let source_text = SourceText::new(source);
        let mut session = oak_core::parser::ParseSession::<RustLanguage>::default();

        let output = oak_core::Builder::build(&builder, &source_text, &[], &mut session);
        output
            .result
            .map_err(|e| NyarError::Parse(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, ast: &RustRoot, ctx: &mut NyarContext<'_, V, A>) -> Id {
        let mut builder = ctx.builder();
        converter::convert_root(ast, &mut builder)
    }
}
