#![warn(missing_docs)]
//! Rusty Nim 语言前端

use nyar_types::{Id, NyarContext, NyarError, NyarFrontend, Vfs};
use oak_core::source::SourceText;
use oak_nim::{NimBuilder, NimLanguage, NimRoot};
use chomsky_uir::ConstraintAnalysis;

/// Rusty Nim 前端
pub struct RustyNimFrontend {
    language: NimLanguage,
}

impl RustyNimFrontend {
    /// 创建新的前端实例
    pub fn new() -> Self {
        Self {
            language: NimLanguage::default(),
        }
    }
}

impl Default for RustyNimFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl NyarFrontend<ConstraintAnalysis, NimRoot> for RustyNimFrontend {
    type Language = NimLanguage;

    fn parse(&self, source: &str) -> Result<NimRoot, NyarError> {
        let builder = NimBuilder::new(&self.language);
        let source_text = SourceText::new(source);
        let mut session = oak_core::parser::ParseSession::<NimLanguage>::default();

        let output = builder.build(&source_text, &[], &mut session);
        output
            .result
            .map_err(|e| NyarError::Parse(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, _ast: &NimRoot, _ctx: &mut NyarContext<'_, V, ConstraintAnalysis>) -> Id {
        // TODO: 实现从 NimRoot 到 IKunTree 的转换
        // ctx.builder().seq(vec![], Default::default())
        0
    }
}
