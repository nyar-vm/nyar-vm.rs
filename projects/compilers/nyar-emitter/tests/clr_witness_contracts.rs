use nyar::{CapabilityTag, Identifier, RewriteTheory, TheoryBundle, WitnessCallEdge, WitnessMethodSlotSubmission, WitnessSubmission};
use nyar_emitter::{
    FragmentSubmission,
    nyar_backend_clr::{
        MsilAssembly, MsilInstruction, MsilInstructionOperand, MsilMethodBody, MsilMethodRef, MsilMethodSignature, MsilModule, MsilOpcode,
        MsilType,
    },
    testing::augment_msil_with_witness,
};

#[test]
fn emits_witness_entry_call_and_console_write() {
    let mut module = MsilModule {
        assembly: MsilAssembly { name: "demo".to_string(), externs: Vec::new() },
        types: Vec::new(),
        global_methods: vec![MsilMethodBody {
            method: MsilMethodRef {
                owner: None,
                name: "Main".to_string(),
                signature: MsilMethodSignature::new(MsilType::Int32 { signed: true }, Vec::new()),
            },
            locals: Vec::new(),
            instructions: vec![
                MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None },
                MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
            ],
            max_stack: 1,
            is_entry_point: true,
            is_async: false,
        }],
    };
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("functions"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("trait-witness")],
        theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
        entry_operation: None,
        external_import_links: Default::default(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        control_flow: None,
        suspend_runtime: None,
        witness_tables: vec![WitnessSubmission {
            type_name: "Dog".to_string(),
            trait_name: "Animal".to_string(),
            table_label: "witness_table_Dog_Animal".to_string(),
            fat_ptr_label: "witness_fat_Dog_Animal".to_string(),
            methods: vec![WitnessMethodSlotSubmission {
                method_name: "make_sound".to_string(),
                impl_symbol: "witness_Dog_Animal_make_sound".to_string(),
                method_index: 0,
            }],
            result_literal: "woof".to_string(),
        }],
        witness_calls: vec![WitnessCallEdge {
            trait_name: "Animal".to_string(),
            type_name: "Dog".to_string(),
            method_index: 0,
            print_result: true,
        }],
        ..Default::default()
    };

    augment_msil_with_witness(&submission, &mut module);
    let entry = module.global_methods.iter().find(|method| method.is_entry_point).expect("entry");
    // 不再生成 `witness_*` 转发桩或占位桩——witness 调用必须直接打到真实 Valkyrie 方法符号。
    // 此 submission 未提供 executable（无真实方法），入口 fallback 到 `impl_symbol`，
    // 由 `reject_unresolved_local_calls` 在完整流程中报编译错误。
    assert!(!module.global_methods.iter().any(|method| method.method.name == "witness_Dog_Animal_make_sound"));
    assert!(entry.instructions.iter().any(|instruction| matches!(
        &instruction.operand,
        Some(MsilInstructionOperand::Method(target)) if target.name == "witness_Dog_Animal_make_sound"
    )));
    assert!(entry.instructions.iter().any(|instruction| matches!(
        &instruction.operand,
        Some(MsilInstructionOperand::Method(target)) if target.name == "WriteLine"
    )));
    assert!(module.assembly.externs.iter().any(|item| item == "System.Console"));
}
