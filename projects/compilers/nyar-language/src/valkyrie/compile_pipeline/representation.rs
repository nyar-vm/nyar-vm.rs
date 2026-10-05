//! CanonicalProgram 到目标无关 RepresentationPlan 的唯一规划器。

use std::collections::BTreeSet;

use nyar_types::{
    CanonicalOperation, CanonicalProgram, CanonicalTypeKind, NominalValueSemantics, StageResult, ValueIdentity,
    layout_choice::{AdtRepresentation, InvokeLowering, RepresentationPlan, SumRepresentation, SumVariantRepresentation, ValueRepresentation},
    pipeline::RepresentationPlanStage,
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
        for nominal in program.linked.nominal_instances.keys() {
            plan.adt_reps.insert(*nominal, AdtRepresentation::TypedAggregate);
        }
        for layout in &program.linked.sum_types {
            if !program.linked.nominal_instances.contains_key(&layout.nominal) {
                return fail_stage(
                    nyar_types::CompileStage::RepresentationPlan,
                    "PLAN003",
                    &program.linked.module_name,
                    format!("sum 布局引用未知 nominal identity: {:?}", layout.nominal),
                );
            }
            let mut variants = std::collections::BTreeMap::new();
            for variant in &layout.variants {
                let Some(record) = program.linked.variants.get(&(layout.nominal, variant.id))
                else {
                    return fail_stage(
                        nyar_types::CompileStage::RepresentationPlan,
                        "PLAN004",
                        &program.linked.module_name,
                        format!("sum 布局引用未知 variant identity: {:?}/{:?}", layout.nominal, variant.id),
                    );
                };
                let payload_type = record.payload_type;
                if payload_type.is_some() != variant.payload_type.is_some() {
                    return fail_stage(
                        nyar_types::CompileStage::RepresentationPlan,
                        "PLAN005",
                        &program.linked.module_name,
                        format!("sum variant payload 合同不一致: {:?}/{:?}", layout.nominal, variant.id),
                    );
                }
                if variants.insert(variant.id, SumVariantRepresentation { id: variant.id, tag: variant.tag, payload_type }).is_some() {
                    return fail_stage(
                        nyar_types::CompileStage::RepresentationPlan,
                        "PLAN006",
                        &program.linked.module_name,
                        format!("sum variant identity 重复: {:?}/{:?}", layout.nominal, variant.id),
                    );
                }
            }
            if plan
                .sum_reps
                .insert(layout.nominal, SumRepresentation { nominal: layout.nominal, tag_width: layout.tag_width, variants })
                .is_some()
            {
                return fail_stage(
                    nyar_types::CompileStage::RepresentationPlan,
                    "PLAN007",
                    &program.linked.module_name,
                    format!("sum nominal identity 重复: {:?}", layout.nominal),
                );
            }
        }
        let nominal_semantics =
            program.linked.nominal_instances.values().map(|record| (record.ty, record.semantics)).collect::<std::collections::BTreeMap<_, _>>();
        let mut instruction_ids = BTreeSet::new();
        for function in program.mir.functions.values() {
            for value in function.value_types.keys() {
                let ty = &program.linked.types[&function.value_types[value]].kind;
                let representation = match ty {
                    CanonicalTypeKind::Nominal { .. } => match nominal_semantics[&function.value_types[value]] {
                        NominalValueSemantics::Value => ValueRepresentation::Specialized,
                        NominalValueSemantics::Reference => ValueRepresentation::Reified,
                    },
                    _ => ValueRepresentation::Specialized,
                };
                plan.value_reps.insert(ValueIdentity::new(function.instance, *value), representation);
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
                        CanonicalOperation::StructNew { .. }
                        | CanonicalOperation::Copy { .. }
                        | CanonicalOperation::AggregateCopy { .. }
                        | CanonicalOperation::LoadConstant { .. }
                        | CanonicalOperation::SumNew { .. }
                        | CanonicalOperation::SumPayloadGet { .. }
                        | CanonicalOperation::SumVariantIs { .. }
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

#[cfg(test)]
mod tests {
    use super::*;
    use nyar_types::{CanonicalProgram, CanonicalSemanticMir};

    #[test]
    fn source_aggregate_semantics_reach_the_representation_plan_without_construction() {
        let output = crate::ValkyrieCompiler::default()
            .compile_source_to_program(
                "structure Point { value: i32 } class Node { value: i32 } \
             micro value_identity(value: Point) -> Point { return value } \
             micro reference_identity(value: Node) -> Node { return value }",
            )
            .expect("声明与参数源码必须完成编译，不依赖构造指令补布局");
        let program = output.canonical();
        let plan = output.representation();
        assert_eq!(program.linked.nominal_instances.len(), 2);
        assert_eq!(plan.adt_reps.len(), 2);
        let semantic_mir = crate::valkyrie::mir::MirLowerer::lower_module_semantic(&crate::ValkyrieCompiler::default().compile_source("structure Point { value: i32 } class Node { value: i32 } micro value_identity(value: Point) -> Point { return value } micro reference_identity(value: Node) -> Node { return value }").expect("source for representation test"));
        for aggregate in &semantic_mir.structs {
            let ty = crate::valkyrie::types::hir::ValkyrieType::Named(crate::valkyrie::types::Identifier::new(&aggregate.name));
            let declaration = semantic_mir.type_identities[&ty];
            let record = program.linked.nominal_instances.values().find(|record| record.declaration == declaration).unwrap();
            let expected = if aggregate.is_value_type { NominalValueSemantics::Value } else { NominalValueSemantics::Reference };
            assert_eq!(record.semantics, expected);
            assert_eq!(record.ty, declaration);
        }
        let mut observed = BTreeSet::new();
        for function in program.mir.functions.values() {
            let (value, ty) = function.parameters[0];
            let CanonicalTypeKind::Nominal { declaration, .. } = &program.linked.types[&ty].kind
            else {
                panic!("源码参数必须保留名义类型");
            };
            let nominal = program.linked.nominal_instances.values().find(|record| record.declaration == *declaration).unwrap();
            let representation = &plan.value_reps[&ValueIdentity::new(function.instance, value)];
            match nominal.semantics {
                NominalValueSemantics::Value => {
                    assert_eq!(*representation, ValueRepresentation::Specialized);
                    observed.insert("value");
                }
                NominalValueSemantics::Reference => {
                    assert_eq!(*representation, ValueRepresentation::Reified);
                    observed.insert("reference");
                }
            }
        }
        assert_eq!(observed, BTreeSet::from(["value", "reference"]));
        let mut invalid = program.clone();
        invalid.linked.nominal_instances.clear();
        invalid.linked.fields.clear();
        let error = CanonicalRepresentationPlanner.plan(&invalid).expect_err("缺名义声明语义不能默认成值聚合");
        assert_eq!(error.records[0].code, "PLAN001");
        assert_eq!(error.records[0].stage, nyar_types::CompileStage::ValidateMir);
    }

    #[test]
    fn planner_requires_each_complete_nominal_type_before_representation() {
        use nyar_types::{NominalInstanceId, SubstitutionId, TypeId, TypeRecord};

        let output = crate::ValkyrieCompiler::default()
            .compile_source_to_program(
                "class Node {} micro first(value: Node, tag: bool) -> Node { return value } \
             micro second(value: Node, tag: i32) -> Node { return value }",
            )
            .expect("源码声明与函数合同必须完成编译");
        let mut program = output.canonical().clone();
        let original = program.linked.nominal_instances.values().next().unwrap().clone();
        let functions = program.mir.functions.keys().copied().collect::<Vec<_>>();
        let mut instantiated_types = Vec::new();
        for (index, function_id) in functions.iter().enumerate() {
            let ty = TypeId::from_index(100 + index as u32).unwrap();
            let nominal = NominalInstanceId::from_index(100 + index as u32).unwrap();
            let function = program.mir.functions.get_mut(function_id).unwrap();
            let argument = function.parameters[1].1;
            program.linked.types.insert(
                ty,
                TypeRecord {
                    declaration: original.declaration,
                    kind: CanonicalTypeKind::Nominal { declaration: original.declaration, arguments: vec![argument] },
                },
            );
            let mut record = original.clone();
            record.ty = ty;
            record.substitution = SubstitutionId::from_index(100 + index as u32).unwrap();
            program.linked.nominal_instances.insert(nominal, record);
            function.parameters[0].1 = ty;
            function.return_type = ty;
            function.value_types.insert(function.parameters[0].0, ty);
            let signature = program.linked.item_instances.get_mut(function_id).unwrap();
            signature.parameter_types[0] = ty;
            signature.return_type = ty;
            instantiated_types.push((nominal, ty));
        }
        let plan = CanonicalRepresentationPlanner.plan(&program).expect("完整实例夹具独立规划，非泛型源码生产证明");
        for (function_id, (nominal, _)) in functions.iter().zip(&instantiated_types) {
            let value = program.mir.functions[function_id].parameters[0].0;
            assert_eq!(plan.value_reps[&ValueIdentity::new(*function_id, value)], ValueRepresentation::Reified);
            assert!(plan.adt_reps.contains_key(nominal));
        }
        program.linked.nominal_instances.remove(&instantiated_types[1].0);
        let error = CanonicalRepresentationPlanner.plan(&program).expect_err("同声明另一个实例不能代替缺失的完整实例");
        assert_eq!(error.records[0].code, "PLAN001");
        assert_eq!(error.records[0].stage, nyar_types::CompileStage::ValidateMir);
    }

    #[test]
    fn planner_accepts_only_validated_canonical_program() {
        let program = CanonicalProgram { linked: Default::default(), mir: CanonicalSemanticMir::default() };
        let plan = CanonicalRepresentationPlanner.plan(&program).expect("空 canonical 程序是合法成功值");
        assert!(plan.invoke_lowerings.is_empty());
    }

    #[test]
    fn source_function_values_keep_their_owners_through_representation_planning() {
        let output = crate::ValkyrieCompiler::default()
            .compile_source_to_program(
                "micro boolean_identity(value: bool) -> bool { return value } \
                 micro integer_identity(value: i32) -> i32 { return value }",
            )
            .expect("源码必须完成前端分析");
        let program = output.canonical();
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
