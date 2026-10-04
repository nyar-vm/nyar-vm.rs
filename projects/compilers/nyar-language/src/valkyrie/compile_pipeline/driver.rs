//! Compiler 在依赖闭包完成后生产唯一的已验证成功载荷。

use nyar_types::{CompiledProgram, CompileStage, StageResult, pipeline::RepresentationPlanStage};

use crate::valkyrie::mir::{MirModule, validation::validate_semantic_module};
use super::{CanonicalRepresentationPlanner, canonical_program_from_semantic_mir, fail_stage};

pub(crate) fn compile_linked_semantic_mir(module: &MirModule) -> StageResult<CompiledProgram> {
    if let Err(error) = validate_semantic_module(module) {
        return fail_stage(
            CompileStage::ValidateMir,
            "PIPE004",
            &module.name,
            format!("Semantic MIR 合同失败: {error:?}"),
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ValkyrieCompiler, mir::{MirLowerer, MirOperation}};

    #[test]
    fn source_without_instance_facts_cannot_mint_identity_from_function_names() {
        let mut hir = ValkyrieCompiler::default().compile_source(
            "micro identity(value: i32) -> i32 { return value }",
        ).expect("源码解析");
        assert!(hir.functions[0].instance.take().is_some());
        let mir = MirLowerer::lower_module_semantic(&hir);
        assert!(mir.callable_identities.is_empty());
        let error = compile_linked_semantic_mir(&mir).expect_err("不能按函数名称补造实例身份");
        assert_eq!(error.records[0].code, "CAN034");
    }

    #[test]
    fn source_calls_and_branch_values_reach_the_success_contract() {
        let output = ValkyrieCompiler::default().compile_source_to_program(
            "micro identity(value: i32) -> i32 { return value } \
             micro choose(flag: bool, value: i32) -> i32 { \
                 if flag { return identity(value) } else { return value } }",
        ).expect("当前源码经过真实构建入口完成语义与表示合同");
        let program = &output;
        assert_eq!(program.canonical().mir.functions.len(), 2);
        assert_eq!(program.representation().invoke_lowerings.len(), 1);
        assert_eq!(
            program.representation().value_reps.len(),
            program.canonical().mir.functions.values().map(|function| function.value_types.len()).sum::<usize>(),
        );
    }

    #[test]
    fn semantic_call_failure_stops_before_canonical_and_representation() {
        let hir = ValkyrieCompiler::default().compile_source(
            "micro identity(value: i32) -> i32 { return value } \
             micro use_identity(value: i32) -> i32 { return identity(value) }",
        ).expect("当前源码解析调用");
        let mut mir = MirLowerer::lower_module_semantic(&hir);
        let arguments = mir.functions.iter_mut()
            .flat_map(|function| &mut function.blocks)
            .flat_map(|block| &mut block.instructions)
            .find_map(|instruction| match &mut instruction.kind {
                MirOperation::Call { arguments, .. } => Some(arguments),
                _ => None,
            }).expect("源码普通调用");
        arguments.clear();
        let error = compile_linked_semantic_mir(&mir).expect_err("不能绕过 Semantic MIR 实参合同");
        assert_eq!(error.records[0].stage, CompileStage::ValidateMir);
        assert_eq!(error.records[0].code, "PIPE004");
    }
}
