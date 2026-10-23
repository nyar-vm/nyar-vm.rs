//! Compiler 在依赖闭包完成后生产唯一的已验证成功载荷。

use nyar_types::{CompileStage, CompiledProgram, StageResult, pipeline::RepresentationPlanStage};

use super::{CanonicalRepresentationPlanner, canonical_program_from_semantic_mir, fail_stage};
use crate::valkyrie::mir::{MirModule, validation::validate_semantic_module};

/// 校验 Semantic MIR 并生产唯一的已验证成功载荷；集成测试夹具。
#[doc(hidden)]
pub(crate) fn compile_linked_semantic_mir(module: &MirModule) -> StageResult<CompiledProgram> {
    if let Err(error) = validate_semantic_module(module) {
        return fail_stage(CompileStage::ValidateMir, "PIPE004", &module.name, format!("Semantic MIR 合同失败: {error:?}"));
    }
    let canonical = canonical_program_from_semantic_mir(module)?;
    let representation = CanonicalRepresentationPlanner.plan(&canonical)?;
    match CompiledProgram::new(canonical, representation) {
        Ok(program) => Ok(program),
        Err(error) => fail_stage(
            CompileStage::RepresentationPlan,
            "PIPE005",
            &module.name,
            format!("CanonicalProgram 与 RepresentationPlan 合同不一致: {error:?}"),
        ),
    }
}
