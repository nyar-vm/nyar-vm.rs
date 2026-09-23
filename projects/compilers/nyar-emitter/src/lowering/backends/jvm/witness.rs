//! Standalone JVM witness lowering (main entry dispatch + impl stubs).

use crate::nyar_backend_jvm::{
    JvmClassFile, JvmCodeBody, JvmInstruction, JvmMethodDescriptor, JvmMethodRef, JvmMethodSignature, JvmTypeDescriptor,
};
use nyar::{WitnessCallEdge, WitnessMethodSlotSubmission, WitnessSubmission};
use std_data::binary::class::JvmFieldRef;

use crate::{FragmentSubmission, lowering::shared::witness_abi::witness_slot_jvm_descriptor};

pub(crate) fn append_witness_methods(class_file: &mut JvmClassFile, submission: &FragmentSubmission) -> Option<Vec<JvmInstruction>> {
    if submission.witness_calls.is_empty() {
        return None;
    }
    emit_witness_impl_methods(class_file, submission);
    Some(lower_witness_main_instructions(submission))
}

pub(crate) fn emit_witness_impl_methods(class_file: &mut JvmClassFile, submission: &FragmentSubmission) {
    for table in &submission.witness_tables {
        for method in &table.methods {
            if class_file.methods.iter().any(|entry| entry.name == method.impl_symbol) {
                continue;
            }
            class_file.methods.push(lower_witness_impl_method(method, table));
        }
    }
}

const JVM_OBJECT: &str = "java/lang/Object";
const JVM_STRING: &str = "java/lang/String";
const JVM_INT_ARRAY: &str = "[I";

fn witness_receiver_int_array_load(index: i32) -> Vec<JvmInstruction> {
    vec![JvmInstruction::ALoad(0), JvmInstruction::CheckCast(JVM_INT_ARRAY.to_string()), JvmInstruction::IConst(index), JvmInstruction::IALoad]
}

fn witness_receiver_int_array_store(index: i32, value: i32) -> Vec<JvmInstruction> {
    vec![
        JvmInstruction::ALoad(0),
        JvmInstruction::CheckCast(JVM_INT_ARRAY.to_string()),
        JvmInstruction::IConst(index),
        JvmInstruction::IConst(value),
        JvmInstruction::IAStore,
    ]
}

fn lower_witness_impl_method(method: &WitnessMethodSlotSubmission, table: &WitnessSubmission) -> JvmMethodSignature {
    if table.trait_name == "Future" && method.method_name == "poll" {
        return lower_future_poll_impl(method);
    }
    if table.trait_name == "Iterator" && method.method_name == "next" {
        return lower_iterator_next_impl(method);
    }

    let descriptor = witness_slot_jvm_descriptor(table, method);
    let instructions = if method.method_index == 0 && !table.result_literal.is_empty() {
        vec![JvmInstruction::LdcString(table.result_literal.clone()), JvmInstruction::AReturn]
    }
    else {
        match &descriptor.return_type {
            JvmTypeDescriptor::Boolean
            | JvmTypeDescriptor::Byte
            | JvmTypeDescriptor::Char
            | JvmTypeDescriptor::Short
            | JvmTypeDescriptor::Int => vec![JvmInstruction::IConst(0), JvmInstruction::IReturn],
            JvmTypeDescriptor::Long => vec![JvmInstruction::LConst0, JvmInstruction::LReturn],
            JvmTypeDescriptor::Float => vec![JvmInstruction::FConst0, JvmInstruction::FReturn],
            JvmTypeDescriptor::Double => vec![JvmInstruction::DConst0, JvmInstruction::DReturn],
            JvmTypeDescriptor::Void => vec![JvmInstruction::Return],
            _ => vec![JvmInstruction::AConstNull, JvmInstruction::AReturn],
        }
    };

    JvmMethodSignature {
        name: method.impl_symbol.clone(),
        descriptor,
        access_flags: 0x0001 | 0x0008,
        code: Some(JvmCodeBody { max_stack: 4, max_locals: 1, instructions }),
    }
}

fn lower_future_poll_impl(method: &WitnessMethodSlotSubmission) -> JvmMethodSignature {
    JvmMethodSignature {
        name: method.impl_symbol.clone(),
        descriptor: JvmMethodDescriptor::new(vec![JvmTypeDescriptor::Object(JVM_OBJECT.to_string())], JvmTypeDescriptor::Boolean),
        access_flags: 0x0001 | 0x0008,
        code: Some(JvmCodeBody {
            max_stack: 4,
            max_locals: 1,
            instructions: {
                let mut instructions = witness_receiver_int_array_load(0);
                instructions.extend([JvmInstruction::IfNe("poll_ready".to_string())]);
                instructions.extend(witness_receiver_int_array_store(0, 1));
                instructions.extend([
                    JvmInstruction::IConst(0),
                    JvmInstruction::IReturn,
                    JvmInstruction::Label("poll_ready".to_string()),
                    JvmInstruction::IConst(1),
                    JvmInstruction::IReturn,
                ]);
                instructions
            },
        }),
    }
}

fn lower_iterator_next_impl(method: &WitnessMethodSlotSubmission) -> JvmMethodSignature {
    JvmMethodSignature {
        name: method.impl_symbol.clone(),
        descriptor: JvmMethodDescriptor::new(
            vec![JvmTypeDescriptor::Object(JVM_OBJECT.to_string())],
            JvmTypeDescriptor::Object(JVM_OBJECT.to_string()),
        ),
        access_flags: 0x0001 | 0x0008,
        code: Some(JvmCodeBody {
            max_stack: 4,
            max_locals: 1,
            instructions: {
                let mut instructions = witness_receiver_int_array_load(0);
                instructions.extend([JvmInstruction::IConst(2), JvmInstruction::IfICmpGe("next_done".to_string())]);
                instructions.extend(witness_receiver_int_array_load(0));
                instructions.extend([JvmInstruction::IConst(0), JvmInstruction::IfICmpEq("next_0".to_string())]);
                instructions.extend(witness_receiver_int_array_load(0));
                instructions.extend([
                    JvmInstruction::IConst(1),
                    JvmInstruction::IfICmpEq("next_1".to_string()),
                    JvmInstruction::Goto("next_done".to_string()),
                    JvmInstruction::Label("next_0".to_string()),
                ]);
                instructions.extend(witness_receiver_int_array_store(0, 1));
                instructions.extend([JvmInstruction::LdcString("0".to_string()), JvmInstruction::AReturn]);
                instructions.push(JvmInstruction::Label("next_1".to_string()));
                instructions.extend(witness_receiver_int_array_store(0, 2));
                instructions.extend([
                    JvmInstruction::LdcString("1".to_string()),
                    JvmInstruction::AReturn,
                    JvmInstruction::Label("next_done".to_string()),
                    JvmInstruction::AConstNull,
                    JvmInstruction::AReturn,
                ]);
                instructions
            },
        }),
    }
}

fn lower_witness_main_instructions(submission: &FragmentSubmission) -> Vec<JvmInstruction> {
    let needs_state = submission.witness_calls.iter().any(|edge| witness_call_needs_stateful_receiver(submission, edge));
    let mut instructions = Vec::new();
    if needs_state {
        instructions.extend([
            JvmInstruction::IConst(1),
            JvmInstruction::NewIntArray,
            JvmInstruction::Dup,
            JvmInstruction::IConst(0),
            JvmInstruction::IConst(0),
            JvmInstruction::IAStore,
            JvmInstruction::AStore(0),
        ]);
    }
    for edge in &submission.witness_calls {
        let Some(slot) = resolve_witness_slot(submission, edge)
        else {
            continue;
        };
        instructions.push(JvmInstruction::GetStatic(JvmFieldRef {
            owner: "java/lang/System".to_string(),
            name: "out".to_string(),
            descriptor: JvmTypeDescriptor::Object("java/io/PrintStream".to_string()),
        }));
        if witness_call_needs_stateful_receiver(submission, edge) {
            instructions.push(JvmInstruction::ALoad(0));
        }
        else {
            instructions.push(JvmInstruction::AConstNull);
        }
        instructions.push(JvmInstruction::InvokeStatic(JvmMethodRef {
            owner: submission_class_owner(submission),
            name: slot.impl_symbol.clone(),
            descriptor: witness_call_descriptor(submission, edge),
        }));
        if edge.print_result {
            instructions.push(JvmInstruction::InvokeVirtual(JvmMethodRef {
                owner: "java/io/PrintStream".to_string(),
                name: "println".to_string(),
                descriptor: JvmMethodDescriptor::new(vec![JvmTypeDescriptor::Object(JVM_OBJECT.to_string())], JvmTypeDescriptor::Void),
            }));
        }
    }
    instructions.push(JvmInstruction::IConst(0));
    instructions.push(JvmInstruction::IReturn);
    instructions
}

fn witness_call_needs_stateful_receiver(submission: &FragmentSubmission, edge: &WitnessCallEdge) -> bool {
    submission.witness_tables.iter().find(|table| table.trait_name == edge.trait_name && table.type_name == edge.type_name).is_some_and(
        |table| {
            (table.trait_name == "Future" && edge.method_index == 0)
                || (table.trait_name == "Iterator"
                    && table.methods.iter().any(|method| method.method_index == edge.method_index && method.method_name == "next"))
        },
    )
}

fn witness_call_descriptor(submission: &FragmentSubmission, edge: &WitnessCallEdge) -> JvmMethodDescriptor {
    let object = JvmTypeDescriptor::Object(JVM_OBJECT.to_string());
    submission
        .witness_tables
        .iter()
        .find(|table| table.trait_name == edge.trait_name && table.type_name == edge.type_name)
        .and_then(|table| table.methods.iter().find(|slot| slot.method_index == edge.method_index).map(|slot| (table, slot)))
        .map(|(table, slot)| witness_slot_jvm_descriptor(table, slot))
        .unwrap_or_else(|| JvmMethodDescriptor::new(vec![object.clone()], object))
}

fn submission_class_owner(submission: &FragmentSubmission) -> String {
    format!("{}/{}", super::sanitize_symbol(&submission.module_name), super::sanitize_symbol(submission.fragment_id.as_str()))
}

fn resolve_witness_slot<'a>(submission: &'a FragmentSubmission, edge: &WitnessCallEdge) -> Option<&'a WitnessMethodSlotSubmission> {
    submission
        .witness_tables
        .iter()
        .find(|table| table.trait_name == edge.trait_name && table.type_name == edge.type_name)
        .and_then(|table| table.methods.iter().find(|slot| slot.method_index == edge.method_index))
}
