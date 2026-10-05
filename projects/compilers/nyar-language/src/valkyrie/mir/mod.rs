#![doc = include_str!("readme.md")]

pub mod singleton;
/// `SSA`-based `MIR` main representation.
pub mod ssa;
pub mod validation;
mod sum;
pub use sum::{MirSumDeclaration, MirSumVariant};

use std_data::text::valkyrie::{ParseError, ValkyrieRoot};

use crate::{hir::ValkyrieCompiler, types::hir::HirModule, validation::ControlFlowScheduler};

pub use singleton::{
    SINGLETON_CONSTRUCTOR_NAME, SINGLETON_EAGER_ACCESSOR, SINGLETON_FINALIZER_NAME, SINGLETON_INSTANCE_FIELD, SINGLETON_LAZY_ACCESSOR,
    SINGLETON_UNLOAD_ACCESSOR, SingletonInstancePlan, SingletonWitnessEntries, collect_aggregate_field_map, collect_singleton_instance_plans,
    collect_singleton_witness_entries, merge_singleton_field_layouts, singleton_accessor_map,
};
pub use ssa::{
    AggregateLayout, AggregateLayoutPlan, ArrayInitialization, FieldLayout, FlagsLayout, LayoutId, MirBlock, MirBlockRef, MirConstant,
    MirDiagnostic, MirEffectKind, MirEntryContract, MirExportContract, MirExternalCallContract, MirField, MirFunction, MirInstruction, MirLowerer, MirModule, MirOperand, MirOperation, MirStorageKind,
    MirStruct, MirTerminator, MirValue, MirValueOrigin, MirValueRef, SumTypeLayout, SumVariantLayout, compute_aggregate_layout_plan,
    layout_id_for_type, layout_key_for_nyar_type, layout_key_for_type, merge_aggregate_layout_plan,
    storage_kind_for_named_type, storage_kind_for_type,
};

impl ValkyrieCompiler {
    /// 将 AST 沿正式 HIR 合同降低为分析用 Semantic MIR，不生产目标产物。
    pub fn lower_root_to_mir(&self, root: &ValkyrieRoot) -> Result<MirModule, ParseError> {
        let hir = self.lower_root(root)?;
        self.validate_hir_semantic_contract(&hir)?;
        self.lower_validated_hir_to_mir(&hir)
    }

    fn lower_validated_hir_to_mir(&self, hir: &HirModule) -> Result<MirModule, ParseError> {
        ControlFlowScheduler::validate_hir_module(hir)?;
        let mir = MirLowerer::lower_module_semantic(hir);
        ControlFlowScheduler::validate_mir_module(&mir)?;
        Ok(mir)
    }

    /// 消费正式源码到 HIR 的展开与验证结果，不另建 parser 路线。
    pub fn compile_source_to_mir(&self, source: &str) -> Result<MirModule, ParseError> {
        let hir = self.compile_source(source)?;
        self.lower_validated_hir_to_mir(&hir)
    }
}

#[cfg(test)]
mod analysis_contract_tests {
    use super::*;
    use std_data::text::valkyrie::AstParser;

    #[test]
    fn source_mir_analysis_rejects_the_same_unresolved_call_as_hir() {
        let compiler = ValkyrieCompiler::default();
        let source = "micro caller() -> i32 { return missing() }";
        let hir_error = compiler.compile_source(source).expect_err("HIR 必须拒绝缺失的调用身份");
        let mir_error = compiler.compile_source_to_mir(source).expect_err("分析入口不得跳过 HIR 调用合同");
        assert!(hir_error.to_string().contains("SMIR003"), "{hir_error}");
        assert_eq!(hir_error.to_string(), mir_error.to_string());
    }

    #[test]
    fn ast_mir_analysis_rejects_unresolved_calls_before_lowering() {
        let root = AstParser::parse_root("micro caller() -> i32 { return missing() }").expect("语法正确的源码");
        let error = ValkyrieCompiler::default().lower_root_to_mir(&root).expect_err("AST 分析入口必须执行 HIR 合同");
        assert!(error.to_string().contains("SMIR003"), "{error}");
    }

    #[test]
    fn source_mir_analysis_preserves_resolved_function_signatures() {
        let compiler = ValkyrieCompiler::default();
        let source = "micro identity(value: i32) -> i32 { return value } micro caller(value: i32) -> i32 { return identity(value) }";
        let hir = compiler.compile_source(source).expect("正式 HIR 调用解析");
        let mir = compiler.compile_source_to_mir(source).expect("相同源码的 MIR 分析");
        assert_eq!(mir.functions.len(), hir.functions.len());
        for (mir_function, hir_function) in mir.functions.iter().zip(&hir.functions) {
            assert_eq!(mir_function.param_types, hir_function.params.iter().map(|parameter| parameter.ty.clone()).collect::<Vec<_>>());
            assert_eq!(mir_function.return_type, hir_function.return_type);
        }
    }
}
