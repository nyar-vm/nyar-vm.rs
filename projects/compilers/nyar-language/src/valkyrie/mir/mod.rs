#![doc = include_str!("readme.md")]

pub mod singleton;
/// `SSA`-based `MIR` main representation.
pub mod ssa;
pub mod validation;

use std_data::text::valkyrie::{AstParser, ParseError, ValkyrieRoot};

use crate::{hir::ValkyrieCompiler, validation::ControlFlowScheduler};

pub use crate::valkyrie::hir::lowering::compute_nominal_layouts;
pub use singleton::{
    SINGLETON_CONSTRUCTOR_NAME, SINGLETON_EAGER_ACCESSOR, SINGLETON_FINALIZER_NAME, SINGLETON_INSTANCE_FIELD, SINGLETON_LAZY_ACCESSOR,
    SINGLETON_UNLOAD_ACCESSOR, SingletonInstancePlan, SingletonWitnessEntries, collect_aggregate_field_map, collect_singleton_instance_plans,
    collect_singleton_return_types, collect_singleton_witness_entries, merge_singleton_field_layouts, singleton_accessor_map,
};
pub use ssa::{
    AggregateLayout, AggregateLayoutPlan, ArrayInitialization, FieldLayout, FlagsLayout, LayoutId, MirBlock, MirBlockRef, MirConstant,
    MirDiagnostic, MirEffectKind, MirField, MirFunction, MirInstruction, MirLowerer, MirModule, MirOperand, MirOperation, MirStorageKind,
    MirStruct, MirTerminator, MirValue, MirValueOrigin, MirValueRef, SumTypeLayout, SumVariantLayout, compute_aggregate_layout_plan,
    layout_id_for_nyar_type, layout_id_for_type, layout_key_for_nyar_type, layout_key_for_type, merge_aggregate_layout_plan,
    storage_kind_for_named_type, storage_kind_for_type,
};

impl ValkyrieCompiler {
    /// Lowers parser output into MIR through the current minimal pipeline.
    pub fn lower_root_to_mir(&self, root: &ValkyrieRoot) -> Result<MirModule, ParseError> {
        let hir = self.lower_root(root)?;
        ControlFlowScheduler::validate_hir_module(&hir)?;
        let mir = MirLowerer::lower_module(&hir);
        ControlFlowScheduler::validate_mir_module(&mir)?;
        Ok(mir)
    }

    /// Parses source text and lowers it into MIR.
    pub fn compile_source_to_mir(&self, source: &str) -> Result<MirModule, ParseError> {
        let root = AstParser::parse_root(source)?;
        self.lower_root_to_mir(&root)
    }
}
