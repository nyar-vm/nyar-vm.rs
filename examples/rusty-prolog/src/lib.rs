#![warn(missing_docs)]

use nyar_types::{Id, Loc, NyarContext, NyarError, NyarFrontend, Vfs};
use oak_prolog::{PrologBuilder, PrologLanguage, PrologRoot};
use oak_core::{Builder, SourceText};

pub struct RustyPrologFrontend {
    language: PrologLanguage,
}

impl RustyPrologFrontend {
    pub fn new() -> Self {
        Self {
            language: PrologLanguage::default(),
        }
    }
}

impl Default for RustyPrologFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl NyarFrontend<(), PrologRoot> for RustyPrologFrontend {
    type Language = PrologLanguage;

    fn parse(&self, source: &str) -> Result<PrologRoot, NyarError> {
        let builder = PrologBuilder::new(self.language);
        let source_text = SourceText::new(source);
        let mut session = oak_core::parser::ParseSession::<PrologLanguage>::default();

        let output = builder.build(&source_text, &[], &mut session);
        output
            .result
            .map_err(|e| NyarError::Parse(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, _ast: &PrologRoot, _ctx: &mut NyarContext<'_, V>) -> Id {
        // TODO: 实现从 PrologRoot 到 IKunTree 的转换
        // _ctx.builder.module("rusty-prolog-program", Vec::new(), Loc::default())
        0
    }
}
