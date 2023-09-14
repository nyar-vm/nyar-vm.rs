//! CanonicalProgram 到目标无关 RepresentationPlan 的唯一规划器。

use std::collections::BTreeSet;

use nyar_types::{
    pipeline::RepresentationPlanStage,
    layout_choice::{AdtRepresentation, InvokeLowering, RepresentationPlan, ValueRepresentation},
    CanonicalOperation, CanonicalProgram, StageResult, ValueIdentity,
};

use super::diagnostics::fail_stage;

/// 只消费 CanonicalProgram 的目标无关表示规划器。
#[derive(Debug, Clone, Copy, Default)]
pub struct CanonicalRepresentationPlanner;

impl RepresentationPlanStage for CanonicalRepresentationPlanner {
    fn plan(&self, program: &CanonicalProgram) -> StageResult<RepresentationPlan> {
        program.validate().map_err(|error| {
            fail_stage::<()>(
                nyar_types::CompileStage::ValidateMir,
                "PLAN001",
                &program.linked.module_name,
                format!("planner 收到未验证的 CanonicalProgram: {error:?}"),
            )
            .err()
            .unwrap()
        })?;

        let mut plan = RepresentationPlan::default();
        let mut instruction_ids = BTreeSet::new();
        for function in program.mir.functions.values() {
            for value in function.value_types.keys() {
                plan.value_reps.insert(ValueIdentity::new(function.instance, *value), value_representation(function, *value));
            }
            for block in function.blocks.values() {
                for instruction in &block.instructions {
                    if !instruction_ids.insert(instruction.id) {
                        return fail_stage(
                            nyar_types::CompileStage::RepresentationPlan,
                            "PLAN002",
                            &program.linked.module_name,
                            format!("InstructionId 重复: {}", instruction.id.index()),
                        );
                    }
                    match &instruction.operation {
                        CanonicalOperation::Invoke { callee, .. } => {
                            let lowering = match callee {
                                nyar_types::CanonicalCallee::Item(_) => InvokeLowering::Direct,
                                nyar_types::CanonicalCallee::Value(_) => InvokeLowering::TypedReference,
                            };
                            plan.invoke_lowerings.insert(instruction.id, lowering);
                        }
                        CanonicalOperation::StructNew { nominal, .. } => {
                            plan.adt_reps.insert(*nominal, AdtRepresentation::TypedAggregate);
                        }
                        CanonicalOperation::Copy { .. }
                        | CanonicalOperation::AggregateCopy { .. }
                        | CanonicalOperation::LoadConstant { .. }
                        | CanonicalOperation::FieldGet { .. }
                        | CanonicalOperation::FieldSet { .. }
                        | CanonicalOperation::ArrayGet { .. }
                        | CanonicalOperation::ArrayNew { .. }
                        | CanonicalOperation::ArrayFromElements { .. }
                        | CanonicalOperation::ArraySet { .. }
                        | CanonicalOperation::ArrayLength { .. }
                        | CanonicalOperation::TupleNew { .. } => {}
                    }
                }
            }
        }
        Ok(plan)
    }
}

fn value_representation(function: &nyar_types::CanonicalFunction, value: nyar_types::MirValueId) -> ValueRepresentation {
    let _ = function.value_types.get(&value);
    ValueRepresentation::Specialized
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyar_types::{CanonicalProgram, CanonicalSemanticMir};

    #[test]
    fn planner_accepts_only_validated_canonical_program() {
        let program = CanonicalProgram { linked: Default::default(), mir: CanonicalSemanticMir::default() };
        let plan = CanonicalRepresentationPlanner.plan(&program).expect("空 canonical 程序是合法成功值");
        assert!(plan.invoke_lowerings.is_empty());
    }

    #[test]
    fn source_function_values_keep_their_owners_through_representation_planning() {
        let output = crate::ValkyrieCompiler::default()
            .compile_source_to_build_output(
                "micro boolean_identity(value: bool) -> bool { return value } \
                 micro integer_identity(value: i32) -> i32 { return value }",
            )
            .expect("源码必须完成前端分析");
        let program = output.canonical_program();
        let plan = CanonicalRepresentationPlanner.plan(&program).expect("完整语义合同必须完成表示规划");
        let functions = program.mir.functions.values().collect::<Vec<_>>();
        assert_eq!(functions.len(), 2);
        let first_value = functions[0].parameters[0].0;
        let second_value = functions[1].parameters[0].0;
        assert_eq!(first_value, second_value);
        assert_ne!(functions[0].parameters[0].1, functions[1].parameters[0].1);
        let expected_values = functions.iter().map(|function| function.value_types.len()).sum::<usize>();
        assert_eq!(plan.value_reps.len(), expected_values);
        for function in &functions {
            for value in function.value_types.keys() {
                assert!(plan.value_reps.contains_key(&ValueIdentity::new(function.instance, *value)));
            }
        }

        let mut invalid = program.clone();
        invalid.mir.functions.get_mut(&functions[1].instance).unwrap().value_types.remove(&second_value);
        let error = CanonicalRepresentationPlanner.plan(&invalid).expect_err("缺类型必须在表示规划前失败");
        assert_eq!(error.records[0].code, "PLAN001");
        assert_eq!(error.records[0].stage, nyar_types::CompileStage::ValidateMir);
    }
}
