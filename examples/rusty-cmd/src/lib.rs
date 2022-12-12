#![warn(missing_docs)]

use chomsky_uir::IKun;
use nyar_types::{Id, NyarContext, NyarError, NyarFrontend};
use oak_cmd::ast::{CmdRoot, Element};
use oak_cmd::{Builder as CmdBuilder, Language as CmdLanguage};
use oak_core::{Builder, SourceText};
use oak_vfs::Vfs;

pub struct RustyCmdFrontend {
    language: CmdLanguage,
}

impl RustyCmdFrontend {
    pub fn new() -> Self {
        Self {
            language: CmdLanguage::default(),
        }
    }

    fn lower_element<V: Vfs>(&self, element: &Element, _ctx: &mut NyarContext<V>) -> Id {
        match element {
            Element::Command(_cmd) => {
                // let callee = ctx.egraph.add(IKun::Symbol(cmd.clone()));
                // ctx.egraph.add(IKun::Apply(callee, vec![]))
                0
            }
            Element::Variable(_v) => 0,
            Element::String(_s) => 0,
            Element::Text(_t) => 0,
            _ => 0,
        }
    }
}

impl Default for RustyCmdFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl NyarFrontend for RustyCmdFrontend {
    type Language = CmdLanguage;

    fn parse(&self, source: &str) -> Result<CmdRoot, NyarError> {
        let builder = CmdBuilder::new(&self.language);
        let source_text = SourceText::new(source);
        let mut session = oak_core::parser::ParseSession::<CmdLanguage>::default();

        let output = builder.build(&source_text, &[], &mut session);
        output
            .result
            .map_err(|e| NyarError::Parse(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, ast: &CmdRoot, ctx: &mut NyarContext<V>) -> Id {
        let mut _ids = vec![];
        for element in &ast.elements {
            _ids.push(self.lower_element(element, ctx));
        }
        // ctx.egraph.add(IKun::Seq(items))
        0
    }
}
