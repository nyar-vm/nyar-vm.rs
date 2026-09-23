#![warn(missing_docs)]

use nyar_types::{Id, Loc, NyarContext, NyarError, NyarFrontend, Vfs};
use oak_scheme::{SchemeBuilder, SchemeLanguage};
use oak_core::{Builder, SourceText};

pub struct RustySchemeFrontend {
    language: SchemeLanguage,
}

impl RustySchemeFrontend {
    pub fn new() -> Self {
        Self {
            language: SchemeLanguage::default(),
        }
    }
}

impl Default for RustySchemeFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl NyarFrontend<(), ()> for RustySchemeFrontend {
    type Language = SchemeLanguage;

    fn parse(&self, source: &str) -> Result<(), NyarError> {
        let builder = SchemeBuilder::new(&self.language);
        let source_text = SourceText::new(source);
        let mut session = oak_core::parser::ParseSession::<SchemeLanguage>::default();

        let output = builder.build(&source_text, &[], &mut session);
        output
            .result
            .map_err(|e| NyarError::Parse(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, _ast: &(), _ctx: &mut NyarContext<'_, V>) -> Id {
        // TODO: 实现从 Scheme AST 到 IKunTree 的转换
        // ctx.builder.module("rusty-scheme-program", Vec::new(), Loc::default())
        0
    }
}
