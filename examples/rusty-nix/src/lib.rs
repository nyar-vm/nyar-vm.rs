use nyar_types::{NyarContext, NyarError, NyarFrontend};
use oak_nix::{NixBuilder, NixLanguage};
use oak_core::{Builder, SourceText};
use oak_vfs::Vfs;
use chomsky_uir::Id;

pub struct RustyNixFrontend {
    language: NixLanguage,
}

impl RustyNixFrontend {
    pub fn new() -> Self {
        Self {
            language: NixLanguage::default(),
        }
    }
}

impl Default for RustyNixFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl NyarFrontend for RustyNixFrontend {
    type Language = NixLanguage;

    fn parse(&self, source: &str) -> Result<(), NyarError> {
        let builder = NixBuilder::new(&self.language);
        let source_text = SourceText::new(source);
        let mut session = oak_core::parser::ParseSession::<NixLanguage>::default();

        let output = builder.build(&source_text, &[], &mut session);
        output
            .result
            .map_err(|e| NyarError::Parse(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, _ast: &(), ctx: &mut NyarContext<V>) -> Id {
        // TODO: 实现从 Nix AST 到 IKunTree 的转换
        ctx.builder.module("rusty-nix-program", Vec::new(), nyar_types::Loc::default())
    }
}
