//! Rusty Elixir 语言前端
//!
//! 这个库提供了 Rusty Elixir 语言的词法分析、语法分析和 Gaia 翻译功能。

#![warn(missing_docs)]
#![feature(new_range_api)]
//! Rusty Elixir 语言前端

pub mod codegen;

use nyar_types::{NyarContext, NyarError, NyarFrontend, Id, Vfs};
use oak_elixir::{ElixirLanguage, ElixirRoot};

/// Rusty Elixir 前端
pub struct RustyElixirFrontend {
    language: ElixirLanguage,
}

impl Default for RustyElixirFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl RustyElixirFrontend {
    /// 创建新的前端实例
    pub fn new() -> Self {
        Self {
            language: ElixirLanguage::default(),
        }
    }
}

impl NyarFrontend<(), ElixirRoot> for RustyElixirFrontend {
    type Language = ElixirLanguage;

    fn parse(&self, source: &str) -> Result<ElixirRoot, NyarError> {
         use oak_core::builder::Builder;
         use oak_elixir::builder::ElixirBuilder;
         
         let builder = ElixirBuilder::new(&self.language);
         let mut cache = oak_core::parser::ParseSession::<ElixirLanguage>::default();
         
         // ElixirBuilder expects a Source + TextEdit
         let result = builder.build(source, &[], &mut cache);
         
         match result.result {
             Ok(root) => Ok(root),
             Err(e) => Err(NyarError::Compile(format!("{:?}", e))),
         }
     }

    fn lower_unified<V: Vfs>(&self, _ast: &ElixirRoot, _ctx: &mut NyarContext<'_, V>) -> Id {
        // codegen::lower_elixir_root(ast, ctx)
        0
    }
}
