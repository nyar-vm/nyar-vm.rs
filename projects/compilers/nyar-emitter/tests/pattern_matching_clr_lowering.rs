//! CLR 后端模式匹配端到端 lowering 测试。
//!
//! 本模块覆盖 Task 7.2：验证 `if let` / `case if` / `while let` / extractor
//! pattern 能正确 lowering 到 CLR MSIL。

use nyar::backends::projection_policy_for_target_profile;
use nyar_emitter::{
    bundled_backend_registry,
    nyar_backend_clr::{MsilInstruction, MsilInstructionOperand, MsilMethodBody, MsilModule, MsilOpcode},
    testing::lower_fragment_to_clr_msil,
};
use nyar_language::{
    CanonicalTarget, ValkyrieCompiler, assemble_fragment_submission, nyar::ClrSuspendStrategy, plan_artifacts_from_build_output,
    types::SourceID,
};

fn compile_source_to_clr_module(source: &str, version_id: u32) -> MsilModule {
    let build_output = ValkyrieCompiler::new(SourceID { version_id })
        .compile_source_to_build_output(source)
        .unwrap_or_else(|error| panic!("compile source failed: {error:?}"));
    let target = CanonicalTarget::parse("clr-microsoft-unknown-managed").expect("clr target");
    let target_profile = target.to_profile(None);
    let projection_policy = projection_policy_for_target_profile(&target_profile).expect("projection policy");
    let registry = bundled_backend_registry(&build_output.neutral_plan().semantic_fragments, &target_profile, &projection_policy);
    let artifact_plan = plan_artifacts_from_build_output(&build_output, target, projection_policy, registry, ClrSuspendStrategy::default())
        .expect("artifact plan");
    let submission = assemble_fragment_submission(&build_output, &artifact_plan, 0).expect("fragment");
    lower_fragment_to_clr_msil(&submission).expect("CLR lowering")
}

fn find_method<'a>(module: &'a MsilModule, name_suffix: &str) -> &'a MsilMethodBody {
    module.global_methods.iter().find(|method| method.method.name.ends_with(name_suffix)).unwrap_or_else(|| {
        panic!(
            "method ending with `{name_suffix}` not found; methods: {:?}",
            module.global_methods.iter().map(|m| &m.method.name).collect::<Vec<_>>()
        )
    })
}

fn is_conditional_branch(instruction: &MsilInstruction) -> bool {
    matches!(
        instruction.opcode,
        MsilOpcode::Brtrue
            | MsilOpcode::Brfalse
            | MsilOpcode::Beq
            | MsilOpcode::BneUn
            | MsilOpcode::Bge
            | MsilOpcode::Bgt
            | MsilOpcode::Ble
            | MsilOpcode::Blt
            | MsilOpcode::BgeUn
            | MsilOpcode::BgtUn
            | MsilOpcode::BleUn
            | MsilOpcode::BltUn
            | MsilOpcode::BrtrueS
            | MsilOpcode::BrfalseS
            | MsilOpcode::BeqS
            | MsilOpcode::BneUnS
            | MsilOpcode::BgeS
            | MsilOpcode::BgtS
            | MsilOpcode::BleS
            | MsilOpcode::BltS
            | MsilOpcode::BgeUnS
            | MsilOpcode::BgtUnS
            | MsilOpcode::BleUnS
            | MsilOpcode::BltUnS
    )
}

fn is_any_branch(instruction: &MsilInstruction) -> bool {
    is_conditional_branch(instruction) || matches!(instruction.opcode, MsilOpcode::Br | MsilOpcode::BrS)
}

fn is_call_to(instruction: &MsilInstruction, needle: &str) -> bool {
    matches!(
        &instruction.operand,
        Some(MsilInstructionOperand::Method(target)) if target.name.contains(needle)
    )
}

fn is_any_call(instruction: &MsilInstruction) -> bool {
    matches!(instruction.opcode, MsilOpcode::Call)
}

#[test]
fn clr_if_let_lowering_emits_branch() {
    let source = r#"unite Option {
    Some { value: i64 }
    None {}
}

micro main(opt: Option) -> i64 {
    if let Some(x) = opt {
        return x
    } else {
        return 0
    }
}
"#;
    let module = compile_source_to_clr_module(source, 7101);
    let main_method = find_method(&module, "main");

    let has_branch = main_method.instructions.iter().any(is_any_branch);
    assert!(has_branch, "if let should emit at least one branch instruction; instructions: {:?}", main_method.instructions);

    let has_extractor_call =
        main_method.instructions.iter().any(|instruction| is_any_call(instruction) && is_call_to(instruction, "extractor"));
    assert!(has_extractor_call, "if let Some(x) should emit an extractor Call; instructions: {:?}", main_method.instructions);
}

#[test]
fn clr_case_if_lowering_emits_arm_check_and_guard() {
    let source = r#"micro main(value: i64) -> i64 {
    case value {
        case 1 if value > 0:
            return value
        else:
            return 0
    }
    return 0
}
"#;
    let module = compile_source_to_clr_module(source, 7102);
    let main_method = find_method(&module, "main");

    let conditional_branch_count = main_method.instructions.iter().filter(|instruction| is_conditional_branch(instruction)).count();
    assert!(
        conditional_branch_count >= 2,
        "case if should emit at least 2 conditional branches (arm check + guard); got {conditional_branch_count}; instructions: {:?}",
        main_method.instructions
    );
}

#[test]
fn clr_while_let_lowering_emits_loop_header() {
    let source = r#"unite Option {
    Some { value: i64 }
    None {}
}

micro main(opt: Option) -> i64 {
    let mut sum = 0
    while let Some(x) = opt {
        sum = sum + x
    }
    return sum
}
"#;
    let module = compile_source_to_clr_module(source, 7103);
    let main_method = find_method(&module, "main");

    let has_back_edge = main_method.instructions.iter().any(|instruction| {
        matches!(instruction.opcode, MsilOpcode::Br | MsilOpcode::BrS)
            && matches!(
                &instruction.operand,
                Some(MsilInstructionOperand::BranchTarget(label)) if label.contains("block_")
            )
    });
    assert!(has_back_edge, "while let should emit a back-edge Br to a loop header label; instructions: {:?}", main_method.instructions);

    let has_extractor_call =
        main_method.instructions.iter().any(|instruction| is_any_call(instruction) && is_call_to(instruction, "extractor"));
    assert!(has_extractor_call, "while let Some(x) should emit an extractor Call; instructions: {:?}", main_method.instructions);
}

#[test]
fn clr_extractor_pattern_lowering_emits_call() {
    let source = r#"unite Option {
    Some { value: i64 }
    None {}
}

micro main(opt: Option) -> i64 {
    case opt {
        case Some(x):
            return x
        else:
            return 0
    }
    return 0
}
"#;
    let module = compile_source_to_clr_module(source, 7104);
    let main_method = find_method(&module, "main");

    let has_extractor_call =
        main_method.instructions.iter().any(|instruction| is_any_call(instruction) && is_call_to(instruction, "extractor"));
    assert!(has_extractor_call, "extractor pattern Some(x) should emit an extractor Call; instructions: {:?}", main_method.instructions);

    let has_conditional_branch = main_method.instructions.iter().any(is_conditional_branch);
    assert!(has_conditional_branch, "extractor pattern case should emit a conditional branch; instructions: {:?}", main_method.instructions);
}

fn stloc_local_index(instruction: &MsilInstruction) -> Option<u16> {
    match instruction.opcode {
        MsilOpcode::Stloc0 => Some(0),
        MsilOpcode::Stloc1 => Some(1),
        MsilOpcode::Stloc2 => Some(2),
        MsilOpcode::Stloc3 => Some(3),
        MsilOpcode::Stloc => match &instruction.operand {
            Some(MsilInstructionOperand::Integer(n)) => Some(*n as u16),
            _ => None,
        },
        _ => None,
    }
}

fn ldloca_local_index(instruction: &MsilInstruction) -> Option<u16> {
    match instruction.opcode {
        MsilOpcode::Ldloca => match &instruction.operand {
            Some(MsilInstructionOperand::Integer(n)) => Some(*n as u16),
            _ => None,
        },
        _ => None,
    }
}

#[test]
fn clr_value_type_binding_emits_field_copy() {
    let source = r#"structure Point {
    x: i64,
    y: i64,
}

unite Wrapper {
    Wrapped { inner: Point }
    Empty {}
}

micro main(w: Wrapper) -> i64 {
    case w {
        case Wrapped(p):
            return p.x + p.y
        else:
            return 0
    }
}
"#;
    let module = compile_source_to_clr_module(source, 7105);
    let main_method = find_method(&module, "main");

    let has_cpblk = main_method.instructions.iter().any(|ins| ins.opcode == MsilOpcode::Cpblk);
    assert!(has_cpblk, "value type binding should emit Cpblk (AggregateCopy); instructions: {:?}", main_method.instructions);

    let ldfld_count = main_method.instructions.iter().filter(|ins| ins.opcode == MsilOpcode::Ldfld).count();
    assert!(
        ldfld_count >= 2,
        "value type field access p.x and p.y should emit at least 2 Ldfld; got {ldfld_count}; instructions: {:?}",
        main_method.instructions
    );
}

#[test]
fn clr_nested_value_type_binding_preserves_layout() {
    let source = r#"structure Pair {
    left: i64,
    right: i64,
}

structure Outer {
    inner: Pair,
}

unite Box {
    Boxed { w: Outer }
    Empty {}
}

micro main(b: Box) -> i64 {
    case b {
        case Boxed(w):
            return w.inner.left
        else:
            return 0
    }
}
"#;
    let module = compile_source_to_clr_module(source, 7106);
    let main_method = find_method(&module, "main");

    let ldfld_fields: Vec<&str> = main_method
        .instructions
        .iter()
        .filter(|ins| ins.opcode == MsilOpcode::Ldfld)
        .filter_map(|ins| match &ins.operand {
            Some(MsilInstructionOperand::Field(_, field)) => Some(field.as_str()),
            _ => None,
        })
        .collect();

    assert!(ldfld_fields.len() >= 2, "nested value type access w.inner.left should emit at least 2 Ldfld; got {:?}", ldfld_fields);

    let inner_pos = ldfld_fields.iter().position(|f| *f == "inner");
    let left_pos = ldfld_fields.iter().position(|f| *f == "left");
    assert!(
        inner_pos.is_some() && left_pos.is_some() && inner_pos < left_pos,
        "field extraction order should be inner before left; got {:?}",
        ldfld_fields
    );
}

#[test]
fn clr_binding_does_not_alias_payload_slot() {
    let source = r#"structure Point {
    x: i64,
    y: i64,
}

unite Wrapper {
    Wrapped { inner: Point }
    Empty {}
}

micro main(w: Wrapper) -> i64 {
    case w {
        case Wrapped(p):
            return p.x
        else:
            return 0
    }
}
"#;
    let module = compile_source_to_clr_module(source, 7107);
    let main_method = find_method(&module, "main");
    let instructions = &main_method.instructions;

    let payload_slot: u16 = instructions
        .iter()
        .enumerate()
        .find(|(_, ins)| is_any_call(ins) && is_call_to(ins, "extractor"))
        .and_then(|(index, _)| instructions.get(index + 1))
        .and_then(stloc_local_index)
        .expect("extractor Call should be followed by stloc (payload slot)");

    let binding_slot: u16 = instructions
        .iter()
        .enumerate()
        .find(|(_, ins)| ins.opcode == MsilOpcode::Cpblk)
        .filter(|(index, _)| *index >= 3)
        .and_then(|(index, _)| instructions.get(index - 3))
        .and_then(ldloca_local_index)
        .expect("Cpblk should be preceded by Ldloca (binding slot)");

    assert_ne!(
        payload_slot, binding_slot,
        "binding slot ({binding_slot}) must not alias payload slot ({payload_slot}); instructions: {:?}",
        instructions
    );
}
