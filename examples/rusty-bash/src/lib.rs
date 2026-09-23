#![warn(missing_docs)]

use chomsky_uir::IKun;
use nyar_types::{Id, NyarContext, NyarError, NyarFrontend};
use oak_bash::ast::{BashRoot, Element};
use oak_bash::{Builder as BashBuilder, Language as BashLanguage};
use oak_core::{Builder, SourceText};
use oak_vfs::Vfs;

pub struct RustyBashFrontend {
    language: BashLanguage,
}

impl RustyBashFrontend {
    pub fn new() -> Self {
        Self {
            language: BashLanguage::default(),
        }
    }

    fn lower_element<V: Vfs>(&self, element: &Element, ctx: &mut NyarContext<'_, V>) -> Id {
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

impl Default for RustyBashFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl NyarFrontend<(), BashRoot> for RustyBashFrontend {
    type Language = BashLanguage;

    fn parse(&self, source: &str) -> Result<BashRoot, NyarError> {
        let builder = BashBuilder::new(&self.language);
        let source_text = SourceText::new(source);
        let mut session = oak_core::parser::ParseSession::<BashLanguage>::default();

        let output = builder.build(&source_text, &[], &mut session);
        output
            .result
            .map_err(|e| NyarError::Parse(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, ast: &BashRoot, ctx: &mut NyarContext<'_, V>) -> Id {
        let mut _ids = vec![];
        for element in &ast.elements {
            _ids.push(self.lower_element(element, ctx));
        }
        // ctx.egraph.add(IKun::Seq(ids))
        0
    }
}
