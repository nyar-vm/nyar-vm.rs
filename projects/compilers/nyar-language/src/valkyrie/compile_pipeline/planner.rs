//! CanonicalProgram 到目标无关 RepresentationPlan 的唯一规划器。

use std::collections::BTreeSet;

use nyar_types::{
    pipeline::RepresentationPlanStage,
    layout_choice::{AdtRepresentation, InvokeLowering, RepresentationPlan, ValueRepresentation},
    CanonicalOperation, CanonicalProgram, StageResult,
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
                plan.value_reps.insert(*value, value_representation(function, *value));
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
                        CanonicalOperation::Invoke { .. } => {
                            plan.invoke_lowerings.insert(instruction.id, InvokeLowering::Direct);
                        }
                        CanonicalOperation::StructNew { nominal, .. } => {
                            plan.adt_reps.insert(*nominal, AdtRepresentation::TypedAggregate);
                        }
                        CanonicalOperation::Copy { .. }
                        | CanonicalOperation::LoadConstant { .. }
                        | CanonicalOperation::FieldGet { .. }
                        | CanonicalOperation::FieldSet { .. }
                        | CanonicalOperation::ArrayGet { .. }
                        | CanonicalOperation::ArraySet { .. }
                        | CanonicalOperation::ArrayLength { .. } => {}
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
}
