use rusty_valkyrie::{
    compile_pipeline::check_instruction_envelope,
    mir::{MirConstant, MirInstruction, MirOperand, MirOperation, MirValueRef},
};
use nyar_types::InstructionId;

#[test]
fn store_var_rejects_results() {
    let insn = MirInstruction {
        id: InstructionId::from_index(0).unwrap(),
        results: vec![MirValueRef(0)],
        kind: MirOperation::StoreVar { name: "x".into(), value: MirOperand::Constant(MirConstant::Int(1)), ty: None },
        provenance: nyar_types::ProvenanceId::from_index(0).unwrap(),
    };
    assert!(check_instruction_envelope("m", &insn).is_err());
}

#[test]
fn load_constant_accepts_one_result() {
    let insn = MirInstruction {
        id: InstructionId::from_index(1).unwrap(),
        results: vec![MirValueRef(0)],
        kind: MirOperation::LoadConstant { constant: MirConstant::Int(1), ty: None },
        provenance: nyar_types::ProvenanceId::from_index(0).unwrap(),
    };
    assert!(check_instruction_envelope("m", &insn).is_ok());
}
