//! Clojure 到 Nyar 字节码的翻译器

use chomsky_cost::DefaultCostModel;
use chomsky_extract::IKunExtractor;
use chomsky_uir::{ConstraintAnalysis, EGraph, IKun, IKunTree, Id, IntentBuilder};
use chomsky_types::Loc;
use nyar_types::NyarError;

pub struct NyarTranslator;

impl NyarTranslator {
    pub fn new() -> Self {
        Self
    }

    pub fn translate_to_tree(&self, _root: &()) -> Result<IKunTree, NyarError> {
        let mut egraph = EGraph::<IKun, ConstraintAnalysis>::new();
        let mut builder = IntentBuilder::new(&mut egraph);
        let root_id = self.translate_to_id(_root, &mut builder)?;
        let extractor = IKunExtractor::new(&egraph, DefaultCostModel);
        Ok(extractor.extract(root_id))
    }

    pub fn translate_to_id(
        &self,
        _root: &(),
        builder: &mut IntentBuilder<ConstraintAnalysis>,
    ) -> Result<Id, NyarError> {
        let root_id = builder.seq(vec![], Loc::default());
        Ok(root_id)
    }
}
