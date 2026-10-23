#![doc = include_str!("readme.md")]

pub mod singleton;
/// `SSA`-based `MIR` main representation.
pub mod ssa;
/// Sum 类型 MIR 声明与变体布局。
mod sum;
/// Semantic MIR 合同与模块级校验。
pub mod validation;
pub use sum::{MirSumDeclaration, MirSumVariant};

use crate::{
    hir::ValkyrieCompiler,
    types::hir::HirModule,
    validation::ControlFlowScheduler,
    valkyrie::{HirError, frontend::ValkyrieRoot},
};

pub use singleton::{
    AggregateFieldIndex, SINGLETON_CONSTRUCTOR_NAME, SINGLETON_EAGER_ACCESSOR, SINGLETON_FINALIZER_NAME, SINGLETON_INSTANCE_FIELD,
    SINGLETON_LAZY_ACCESSOR, SINGLETON_UNLOAD_ACCESSOR, SingletonInstancePlan, SingletonWitnessEntries, collect_aggregate_field_map,
    collect_singleton_accessor_contracts, collect_singleton_instance_plans, collect_singleton_witness_entries, merge_singleton_field_layouts,
    singleton_accessor_instance_map, singleton_accessor_map,
};
pub use ssa::{
    AggregateLayout, AggregateLayoutPlan, ArrayInitialization, FieldLayout, FlagsLayout, LayoutId, LayoutKey, MirBlock, MirBlockRef,
    MirConstant, MirDiagnostic, MirEffectKind, MirEntryContract, MirExportContract, MirExternalCallContract, MirField, MirFunction,
    MirInstruction, MirLowerer, MirModule, MirOperand, MirOperation, MirStorageKind, MirStruct, MirTerminator, MirValue, MirValueOrigin,
    MirValueRef, SumTypeLayout, SumVariantLayout, compute_aggregate_layout_plan, layout_id_for_type, storage_kind_for_type,
};

impl ValkyrieCompiler {
    /// 集成测试夹具：将预解析 AST 降低为 Semantic MIR，不进入正式 Compiler 入口。
    #[doc(hidden)]
    pub(crate) fn lower_root_to_mir(&self, root: &ValkyrieRoot) -> Result<MirModule, HirError> {
        let hir = self.lower_root(root)?;
        self.validate_hir_semantic_contract(&hir)?;
        self.lower_validated_hir_to_mir(&hir)
    }

    fn lower_validated_hir_to_mir(&self, hir: &HirModule) -> Result<MirModule, HirError> {
        ControlFlowScheduler::validate_hir_module(hir)?;
        let mir = MirLowerer::lower_module_semantic(hir);
        ControlFlowScheduler::validate_mir_module(&mir)?;
        Ok(mir)
    }

    /// 集成测试夹具：从单文件源码降低到 Semantic MIR；正式 Compiler 入口只接受源码闭包。
    #[doc(hidden)]
    pub(crate) fn compile_source_to_mir(&self, source: &str) -> Result<MirModule, HirError> {
        let hir = self.compile_source(source)?;
        self.lower_validated_hir_to_mir(&hir)
    }
}
