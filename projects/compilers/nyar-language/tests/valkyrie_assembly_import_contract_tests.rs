use nyar::{ArtifactPartitionPlan, CanonicalTarget, ClrSuspendStrategy};
use nyar_language::{
    ValkyrieCompiler,
    valkyrie::assembly::{assemble_fragment, build_output_surface_counts, plan_artifacts_from_compiled_program},
};
use nyar_types::CompiledProgram;

#[test]
fn source_entry_survives_canonical_partition_planning() {
    let output = nyar_language::ValkyrieCompiler::default()
        .compile_source_to_program(
            "[export(name: \"first\")] [main] micro first() -> unit { return } \
             [export(name: \"second\")] micro second() -> unit { return }",
        )
        .expect("当前源码必须形成完整成功载荷");
    let program = &output;
    let plan = plan_artifacts_from_compiled_program(program, CanonicalTarget::wasm(), ClrSuspendStrategy::default())
        .expect("Compiler 必须从 Canonical 身份产生分区");
    assert!(plan.partitions.iter().any(|partition| partition.entry_operation.is_some()));
}

#[test]
fn function_without_entry_remains_a_non_entry_partition() {
    let output = nyar_language::ValkyrieCompiler::default()
        .compile_source_to_program("[primitive(\"core::primitive::i32\")] structure i32 { } micro answer() -> i32 { return 23 }")
        .expect("当前源码必须编译");
    let plan = plan_artifacts_from_compiled_program(&output, CanonicalTarget::wasm(), ClrSuspendStrategy::default())
        .expect("无入口函数仍可形成明确的非入口分区");
    assert!(plan.partitions.iter().all(|partition| partition.entry_operation.is_none()));
}

#[test]
fn compiler_surface_counts_consume_verified_canonical_exports() {
    let output = nyar_language::ValkyrieCompiler::default()
        .compile_source_to_program(
            "[export(name: \"first\")] [main] micro first() -> unit { return }\n[export(name: \"second\")] micro second() -> unit { return }",
        )
        .expect("多导出源码必须形成完整成功载荷");
    assert_eq!(build_output_surface_counts(&output), (2, 1));
    let linked = &output.canonical().linked;
    assert_eq!(linked.exports.len(), 2);
    assert!(linked.exports.keys().all(|instance| output.canonical().mir.functions.contains_key(instance)));
}

#[test]
fn compiler_surface_rejects_duplicate_public_names_before_assembly() {
    let error = nyar_language::ValkyrieCompiler::default()
        .compile_source_to_program(
            "[export(name: \"same\")] micro first() -> unit { return }\n[export(name: \"same\")] micro second() -> unit { return }",
        )
        .expect_err("重复公开名不能进入装配");
    assert!(error.to_string().contains("DuplicateExportName"), "{error}");
}

fn source_partition_contract() -> (CompiledProgram, ArtifactPartitionPlan) {
    let program = nyar_language::ValkyrieCompiler::default()
        .compile_source_to_program(
            "[export(name: \"first\")] [main] micro first() -> unit { return } \
             [export(name: \"second\")] micro second() -> unit { return }",
        )
        .expect("当前多导出源码必须形成完整成功载荷");
    let plan = plan_artifacts_from_compiled_program(&program, CanonicalTarget::wasm(), ClrSuspendStrategy::default())
        .expect("分区必须消费已验证的 Canonical 合同");
    (program, plan)
}

#[test]
fn partition_identity_survives_planning_and_assembly() {
    let (program, plan) = source_partition_contract();
    assert!(!plan.partitions.is_empty());
    for (index, partition) in plan.partitions.iter().enumerate() {
        let fragment = program.canonical().linked.fragments.get(&partition.fragment).expect("Canonical 片段");
        assert_eq!(partition.exported_operations, fragment.exported_operations);
        assert_eq!(partition.entry_operation, fragment.entry_operation);
        let assembled = assemble_fragment(&program, &plan, index).expect("身份一致的分区必须装配成功");
        assert_eq!(assembled.partition.fragment, partition.fragment);
        assert_eq!(assembled.partition.exported_operations, fragment.exported_operations);
    }
}

#[test]
fn assembly_rejects_changed_partition_and_view_identities() {
    let (program, plan) = source_partition_contract();
    for mutation in 0..3 {
        let mut changed = plan.clone();
        match mutation {
            0 => changed.partitions[0].exported_operations.clear(),
            1 => changed.partitions[0].entry_operation = None,
            _ => {
                let identity = changed.partitions[0].fragment.clone();
                changed.fragment_views.iter_mut().find(|view| view.fragment_id == identity).expect("片段视图").canonical_operations.clear();
            }
        }
        let error = assemble_fragment(&program, &changed, 0).expect_err("不一致身份不得被 Canonical 原始根兜底掩盖");
        assert!(error.to_string().contains("callable 身份与 Canonical 片段合同不一致"), "{error}");
    }
}
