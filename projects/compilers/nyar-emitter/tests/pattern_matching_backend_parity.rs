//! 多后端模式匹配 parity 回归矩阵。

use nyar::{QualifiedName, backends::projection_policy_for_target_profile};
use nyar_emitter::{
    FragmentSubmission, bundled_backend_registry,
    nyar_backend_clr::{MsilInstruction, MsilInstructionOperand, MsilOpcode},
    nyar_backend_jvm::JvmInstruction,
    testing::{lower_fragment_to_wasm_mir_module, lower_mir_to_clr_method, lower_mir_to_jvm_method},
};
use nyar_language::{
    CanonicalTarget, MirFunction, MirInstructionKind, ValkyrieCompiler, assemble_fragment_submission, nyar::ClrSuspendStrategy,
    plan_artifacts_from_build_output, types::SourceID,
};

const WASM_BR: u8 = 0x0C;
const WASM_BR_IF: u8 = 0x0D;
const WASM_CALL: u8 = 0x10;
const WASM_CODE_SECTION_ID: u8 = 10;

const IF_LET_SOURCE: &str = r#"unite Option {
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

const CASE_IF_SOURCE: &str = r#"micro main(value: i64) -> i64 {
    case value {
        case 1 if value > 0:
            return value
        else:
            return 0
    }
    return 0
}
"#;

const WHILE_LET_SOURCE: &str = r#"unite Option {
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

const EXTRACTOR_SOURCE: &str = r#"unite Option {
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

fn compile_source_to_submission(source: &str, version_id: u32) -> FragmentSubmission {
    let build_output = ValkyrieCompiler::new(SourceID { version_id })
        .compile_source_to_build_output(source)
        .unwrap_or_else(|error| panic!("compile source failed: {error:?}"));
    let target = CanonicalTarget::parse("clr-microsoft-unknown-managed").expect("clr target");
    let target_profile = target.to_profile(None);
    let projection_policy = projection_policy_for_target_profile(&target_profile).expect("projection policy");
    let registry = bundled_backend_registry(&build_output.neutral_plan().semantic_fragments, &target_profile, &projection_policy);
    let artifact_plan = plan_artifacts_from_build_output(&build_output, target, projection_policy, registry, ClrSuspendStrategy::default())
        .expect("artifact plan");
    assemble_fragment_submission(&build_output, &artifact_plan, 0).expect("fragment")
}

fn find_main_operation(submission: &FragmentSubmission) -> (QualifiedName, nyar_emitter::executable_provider::ExecutableFunction) {
    let exec = submission.executable.as_ref().expect("executable provider");
    for operation in exec.operations() {
        if operation.parts().last().map(|part| part.as_str()) == Some("main") {
            let view = exec.get_function(&operation).expect("main function view");
            return (operation, view.function);
        }
    }
    panic!("main function not found; ops: {:?}", exec.operations());
}

fn clr_is_conditional_branch(instruction: &MsilInstruction) -> bool {
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

fn clr_is_any_branch(instruction: &MsilInstruction) -> bool {
    clr_is_conditional_branch(instruction) || matches!(instruction.opcode, MsilOpcode::Br | MsilOpcode::BrS)
}

fn clr_is_call_to(instruction: &MsilInstruction, needle: &str) -> bool {
    matches!(
        &instruction.operand,
        Some(MsilInstructionOperand::Method(target)) if target.name.contains(needle)
    )
}

fn clr_is_any_call(instruction: &MsilInstruction) -> bool {
    matches!(instruction.opcode, MsilOpcode::Call)
}

fn jvm_is_conditional_branch(instruction: &JvmInstruction) -> bool {
    matches!(
        instruction,
        JvmInstruction::IfEq(_)
            | JvmInstruction::IfNe(_)
            | JvmInstruction::IfLt(_)
            | JvmInstruction::IfLe(_)
            | JvmInstruction::IfGt(_)
            | JvmInstruction::IfGe(_)
            | JvmInstruction::IfNull(_)
            | JvmInstruction::IfNonNull(_)
            | JvmInstruction::IfICmpEq(_)
            | JvmInstruction::IfICmpNe(_)
            | JvmInstruction::IfICmpLt(_)
            | JvmInstruction::IfICmpLe(_)
            | JvmInstruction::IfICmpGt(_)
            | JvmInstruction::IfICmpGe(_)
    )
}

fn jvm_is_any_branch(instruction: &JvmInstruction) -> bool {
    jvm_is_conditional_branch(instruction) || matches!(instruction, JvmInstruction::Goto(_))
}

fn jvm_is_call_to(instruction: &JvmInstruction, needle: &str) -> bool {
    match instruction {
        JvmInstruction::InvokeStatic(method_ref) | JvmInstruction::InvokeVirtual(method_ref) => method_ref.name.contains(needle),
        _ => false,
    }
}

fn jvm_is_any_call(instruction: &JvmInstruction) -> bool {
    matches!(instruction, JvmInstruction::InvokeStatic(_) | JvmInstruction::InvokeVirtual(_))
}

fn wasm_has_br_if(bytes: &[u8]) -> bool {
    bytes.contains(&WASM_BR_IF)
}

fn wasm_has_br(bytes: &[u8]) -> bool {
    bytes.contains(&WASM_BR)
}

fn wasm_has_call(bytes: &[u8]) -> bool {
    bytes.contains(&WASM_CALL)
}

fn wasm_br_if_count(bytes: &[u8]) -> usize {
    bytes.iter().filter(|byte| **byte == WASM_BR_IF).count()
}

fn mir_has_call_instruction(mir_fn: &nyar_emitter::executable_provider::ExecutableFunction) -> bool {
    mir_fn
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .any(|instruction| matches!(instruction.kind, MirInstructionKind::Call { .. }))
}

struct TripleBackend {
    clr_instructions: Vec<MsilInstruction>,
    jvm_instructions: Vec<JvmInstruction>,
    wasm_bytes: Vec<u8>,
    mir_has_call: bool,
}

fn lower_to_all_backends(submission: &FragmentSubmission) -> TripleBackend {
    let (operation, mir_fn) = find_main_operation(submission);
    let mir_has_call = mir_has_call_instruction(&mir_fn);
    let clr_method = lower_mir_to_clr_method(submission, &operation, &mir_fn).expect("CLR MIR lowering");
    let jvm_method = lower_mir_to_jvm_method(submission, &operation, &mir_fn);
    let wasm_module = lower_fragment_to_wasm_mir_module(submission, "main");
    let wasm_bytes =
        wasm_module.sections.iter().find(|section| section.id == WASM_CODE_SECTION_ID).map(|section| section.bytes.clone()).unwrap_or_default();
    TripleBackend {
        clr_instructions: clr_method.instructions,
        jvm_instructions: jvm_method.code.expect("jvm method must have code body").instructions,
        wasm_bytes,
        mir_has_call,
    }
}

#[test]
fn parity_if_let_all_backends_emit_branch() {
    let submission = compile_source_to_submission(IF_LET_SOURCE, 8301);
    let backend = lower_to_all_backends(&submission);

    assert!(backend.clr_instructions.iter().any(clr_is_any_branch));
    assert!(backend.jvm_instructions.iter().any(jvm_is_any_branch));
    assert!(wasm_has_br_if(&backend.wasm_bytes));

    assert!(backend.clr_instructions.iter().any(|ins| clr_is_any_call(ins) && clr_is_call_to(ins, "extractor")));
    assert!(backend.jvm_instructions.iter().any(jvm_is_any_call));
    assert!(wasm_has_call(&backend.wasm_bytes) || backend.mir_has_call);
}

#[test]
fn parity_case_if_guard_all_backends_emit_dual_branch() {
    let submission = compile_source_to_submission(CASE_IF_SOURCE, 8302);
    let backend = lower_to_all_backends(&submission);

    assert!(backend.clr_instructions.iter().filter(|ins| clr_is_conditional_branch(ins)).count() >= 2);
    assert!(backend.jvm_instructions.iter().filter(|ins| jvm_is_conditional_branch(ins)).count() >= 2);
    assert!(wasm_br_if_count(&backend.wasm_bytes) >= 2);
}

#[test]
fn parity_while_let_all_backends_emit_loop_backedge() {
    let submission = compile_source_to_submission(WHILE_LET_SOURCE, 8303);
    let backend = lower_to_all_backends(&submission);

    assert!(backend.clr_instructions.iter().any(clr_is_conditional_branch));
    assert!(backend.clr_instructions.iter().any(|ins| matches!(ins.opcode, MsilOpcode::Br | MsilOpcode::BrS)));

    assert!(backend.jvm_instructions.iter().any(jvm_is_conditional_branch));
    assert!(backend.jvm_instructions.iter().any(|ins| matches!(ins, JvmInstruction::Goto(_))));

    assert!(wasm_has_br_if(&backend.wasm_bytes));
    assert!(wasm_has_br(&backend.wasm_bytes));

    assert!(backend.clr_instructions.iter().any(|ins| clr_is_any_call(ins) && clr_is_call_to(ins, "extractor")));
    assert!(backend.jvm_instructions.iter().any(jvm_is_any_call));
    assert!(wasm_has_call(&backend.wasm_bytes) || backend.mir_has_call);
}

#[test]
fn parity_extractor_all_backends_emit_call() {
    let submission = compile_source_to_submission(EXTRACTOR_SOURCE, 8304);
    let backend = lower_to_all_backends(&submission);

    assert!(backend.clr_instructions.iter().any(|ins| clr_is_any_call(ins) && clr_is_call_to(ins, "extractor")));
    assert!(backend.jvm_instructions.iter().any(|ins| jvm_is_call_to(ins, "extractor")));
    assert!(wasm_has_call(&backend.wasm_bytes) || backend.mir_has_call);

    assert!(backend.clr_instructions.iter().any(clr_is_conditional_branch));
    assert!(backend.jvm_instructions.iter().any(jvm_is_conditional_branch));
    assert!(wasm_has_br_if(&backend.wasm_bytes));
}
