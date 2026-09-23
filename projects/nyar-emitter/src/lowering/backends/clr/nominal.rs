use nyar_types::{FlagsLayout, SumTypeLayout};
use std_data::text::msil::{
    MsilField, MsilInstruction, MsilInstructionOperand, MsilMethodBody, MsilMethodRef, MsilMethodSignature, MsilOpcode, MsilType, MsilTypeDef,
};

fn object_ctor_ref() -> MsilMethodRef {
    MsilMethodRef {
        owner: Some("[mscorlib]System.Object".to_string()),
        name: ".ctor".to_string(),
        signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
    }
}

fn default_reference_ctor(type_name: &str) -> MsilMethodBody {
    MsilMethodBody {
        method: MsilMethodRef {
            owner: Some(type_name.to_string()),
            name: ".ctor".to_string(),
            signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
        },
        locals: Vec::new(),
        instructions: vec![
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Call, operand: Some(MsilInstructionOperand::Method(object_ctor_ref())) },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

/// Build CLR type definitions for sum and flags nominal layouts.
///
/// - Ordinary enums → value types (`tag` + `payload`)
/// - Unite sums (`Result` / `Option`, …) → reference classes: MIR maps `Apply(Result,…)` to
///   `Object`, and `Fine`/`Fail` constructors allocate via `newobj`.
pub(crate) fn build_clr_nominal_type_defs(sum_types: &[SumTypeLayout], flags_types: &[FlagsLayout]) -> Vec<MsilTypeDef> {
    let mut defs = Vec::new();
    for sum in sum_types {
        let is_value_type = !sum.is_unite;
        defs.push(MsilTypeDef {
            full_name: sum.name.clone(),
            namespace: String::new(),
            fields: vec![
                MsilField { name: "tag".to_string(), ty: MsilType::Int32 { signed: true }, is_static: false },
                MsilField { name: "payload".to_string(), ty: MsilType::Object, is_static: false },
            ],
            methods: if is_value_type { Vec::new() } else { vec![default_reference_ctor(&sum.name)] },
            is_value_type,
        });
    }
    for flags in flags_types {
        defs.push(MsilTypeDef {
            full_name: flags.name.clone(),
            namespace: String::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            is_value_type: true,
        });
    }
    defs
}
