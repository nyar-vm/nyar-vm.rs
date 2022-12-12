use crate::{
    executable_provider::{
        ExecutableFunction as MirFunction, ExecutableInstructionKind as MirInstructionKind, ExecutableOperand as MirOperand,
        ExecutableProvider, ExecutableValueRef as MirValueRef, NyarType,
    },
    nyar_backend_clr::{
        MsilAssembly, MsilInstruction, MsilInstructionOperand, MsilMethodBody, MsilMethodRef, MsilMethodSignature, MsilModule, MsilOpcode,
        MsilType,
    },
};
use miette::{Result, miette};
use nyar::{Identifier, NamePath, QualifiedName};

use super::{
    clr_mir::{lower_mir_function_to_msil, synthesize_array_ordinal_host_standalone},
    clr_nominal::build_clr_nominal_type_defs,
    clr_types::{build_clr_type_defs, nyar_type_to_msil},
    interop::clr_host_method_target,
    sanitize_operation_symbol, sanitize_symbol,
    witness_abi::{INJECTED_RUNTIME_STUBS, is_tuple_get_stub_name},
};
use crate::{
    FragmentSubmission,
    lowering::tooling::clr_cli::{is_cli_entry_main, lower_entry_with_cli_args},
};
use std_data::text::msil::MsilTypeDef;

fn merge_clr_type_defs(submission: &FragmentSubmission) -> Vec<MsilTypeDef> {
    let mut types = build_clr_type_defs(&submission.aggregate_layouts);
    let mut seen =
        types.iter().map(|type_def| (type_def.namespace.clone(), type_def.full_name.clone())).collect::<std::collections::BTreeSet<_>>();
    for type_def in build_clr_nominal_type_defs(&submission.sum_types, &submission.flags_types) {
        let key = (type_def.namespace.clone(), type_def.full_name.clone());
        if seen.insert(key) {
            types.push(type_def);
        }
    }
    types
}

/// Non-unite enums (`enums VonTokenKind { … }`) are CLR valuetypes (`tag` + `payload`), but they
/// are carried in `sum_types` rather than `aggregate_layouts`. Without registering their names in
/// `value_type_names`, `nyar_type_to_msil` maps fields like `VonToken.kind` to `object`, the lexer
/// boxes the enum, and `match token.kind` does `ldloca`+`ldfld tag` on an object local — reading
/// garbage tags → `当前位置不能解析为 VON 值` on valid `{ … }` documents.
fn register_clr_enum_value_types(submission: &mut FragmentSubmission) {
    for sum in &submission.sum_types {
        if !sum.is_unite {
            submission.aggregate_layouts.value_type_names.insert(sum.name.clone());
        }
    }
}

pub(crate) fn lower_fragment_to_msil(submission: &FragmentSubmission) -> Result<MsilModule> {
    crate::lowering::features::semantic_mir_contract::validate_submission(submission).map_err(|error| {
        miette::miette!("semantic MIR contract failed [{}] {} at {}: {}", error.code, error.function, error.location, error.detail)
    })?;
    crate::lowering::features::physical_contract::validate_physical_submission(
        submission,
        crate::lowering::features::physical_contract::PhysicalBackend::Clr,
    )
    .map_err(|error| miette::miette!("physical contract failed [{}] {} at {}: {}", error.code, error.function, error.location, error.detail))?;
    let mut submission = submission.clone();
    register_clr_enum_value_types(&mut submission);
    let submission = &submission;
    let mut local_operations = submission.exported_operations.clone();
    for edge in &submission.internal_call_edges {
        if !local_operations.iter().any(|operation| operation == &edge.caller) {
            local_operations.push(edge.caller.clone());
        }
        if !local_operations.iter().any(|operation| operation == &edge.callee_symbol) {
            local_operations.push(edge.callee_symbol.clone());
        }
    }
    for edge in &submission.external_call_edges {
        if !local_operations.iter().any(|operation| operation == &edge.caller) {
            local_operations.push(edge.caller.clone());
        }
        if !local_operations.iter().any(|operation| operation == &edge.callee_symbol) {
            local_operations.push(edge.callee_symbol.clone());
        }
    }
    for operation in submission.operation_literal_returns.keys() {
        if !local_operations.iter().any(|existing| existing == operation) {
            local_operations.push(operation.clone());
        }
    }
    if let Some(entry_operation) = submission.entry_operation.as_ref() {
        if !local_operations.iter().any(|operation| operation == entry_operation) {
            local_operations.push(entry_operation.clone());
        }
    }
    if let Some(executable) = submission.executable.as_ref() {
        for operation in executable.operations() {
            if !local_operations.iter().any(|existing| existing == &operation) {
                local_operations.push(operation);
            }
        }
    }
    expand_operations_with_mir_callees(submission, &mut local_operations);
    // Witness 表里的 impl 方法（`imply Type: Trait { micro method }`）在 MIR 层
    // 已经按 `{Type}.{method}` 约定降级为独立函数。这里把它们的 operation 加入
    // 发出列表，确保 CLR 后端真正发出用户写的 Valkyrie 方法体，而不是只靠
    // `witness.rs` 里的 Rust mock 桩。
    for table in &submission.witness_tables {
        for method in &table.methods {
            let operation = QualifiedName::new(vec![Identifier::new(&table.type_name), Identifier::new(&method.method_name)]);
            if !local_operations.iter().any(|existing| existing == &operation) {
                local_operations.push(operation);
            }
        }
    }
    // Ordinal Array.get/set: host_contract rebinding may leave only an external link.
    for name in submission.external_import_links.keys() {
        if is_array_ordinal_host_operation(name).is_some() && !local_operations.iter().any(|existing| existing == name) {
            local_operations.push(name.clone());
        }
    }

    let mut global_methods = local_operations
        .iter()
        .filter(|operation| !is_direct_clr_import(submission, operation))
        .map(|operation| lower_operation_method(submission, operation))
        .collect::<Result<Vec<_>>>()?;
    let legion_cli = submission.entry_operation.as_ref().is_some_and(|entry| is_cli_entry_main(submission, entry));
    let entry_name = if legion_cli {
        "Main".to_string()
    }
    else {
        submission
            .entry_operation
            .as_ref()
            .map(|entry| format!("entry_{}", sanitize_operation_symbol(entry)))
            .unwrap_or_else(|| "Main".to_string())
    };
    let entry_return_type = if legion_cli {
        MsilType::Int32 { signed: true }
    }
    else {
        submission
            .entry_operation
            .as_ref()
            .and_then(|entry| if submission.operation_void_returns.contains(entry) { Some(MsilType::Void) } else { None })
            .unwrap_or(MsilType::Int32 { signed: true })
    };
    let entry_params = if legion_cli { vec![MsilType::sz_array(MsilType::String)] } else { Vec::new() };
    let entry_instructions = if legion_cli {
        let entry_operation = submission.entry_operation.as_ref().expect("legion cli requires entry operation");
        let signature = method_signature_for(submission, entry_operation);
        lower_entry_with_cli_args(entry_operation, signature)
    }
    else {
        lower_entry_instructions(submission, submission.entry_operation.as_ref())
    };
    let entry_method = MsilMethodBody {
        method: MsilMethodRef { owner: None, name: entry_name, signature: MsilMethodSignature::new(entry_return_type, entry_params) },
        locals: Vec::new(),
        instructions: entry_instructions,
        max_stack: 8,
        is_entry_point: true,
        is_async: false,
    };
    ensure_runtime_stubs(&mut global_methods);
    ensure_scalar_text_runtime_helpers(&mut global_methods);
    ensure_referenced_array_ordinal_helpers(&mut global_methods);
    global_methods.insert(0, entry_method);
    let types = merge_clr_type_defs(submission);
    Ok(MsilModule {
        assembly: MsilAssembly {
            name: format!("{}__{}", sanitize_symbol(&submission.module_name), sanitize_symbol(submission.fragment_id.as_str())),
            externs: collect_clr_externs(submission),
        },
        types,
        global_methods,
    })
}

fn operation_returns_void(submission: &FragmentSubmission, operation: &QualifiedName) -> bool {
    submission.operation_void_returns.contains(operation)
}

fn msil_return_type(submission: &FragmentSubmission, operation: &QualifiedName) -> MsilType {
    if operation_returns_void(submission, operation) {
        MsilType::Void
    }
    else {
        submission
            .executable
            .as_ref()
            .and_then(|exec| exec.get_function(operation))
            .map(|view| nyar_type_to_msil(&view.function.return_type, &submission.aggregate_layouts))
            .unwrap_or(MsilType::Int32 { signed: true })
    }
}

fn method_signature_for(submission: &FragmentSubmission, operation: &QualifiedName) -> MsilMethodSignature {
    let param_types = submission
        .executable
        .as_ref()
        .and_then(|exec| exec.get_function(operation))
        .map(|view| view.function.param_types.iter().map(|ty| nyar_type_to_msil(ty, &submission.aggregate_layouts)).collect::<Vec<_>>())
        .unwrap_or_default();
    MsilMethodSignature::new(msil_return_type(submission, operation), param_types)
}

/// The CLR representation is `System.String` (UTF-16), while language `utf8`
/// indexing is in Unicode scalar values. Keep the conversion in one injected
/// physical helper selected only by the structured Semantic MIR opcode.
fn ensure_scalar_text_runtime_helpers(global_methods: &mut Vec<MsilMethodBody>) {
    if !global_methods.iter().any(|method| method.method.name == "__nyar_utf8_scalar_length") {
        global_methods.push(utf8_scalar_length_runtime_helper());
    }
}

fn utf8_scalar_length_runtime_helper() -> MsilMethodBody {
    let i32 = MsilType::Int32 { signed: true };
    let char_is_high = MsilMethodRef {
        owner: Some("[mscorlib]System.Char".to_string()),
        name: "IsHighSurrogate".to_string(),
        signature: MsilMethodSignature::new(MsilType::Bool, vec![MsilType::Char]),
    };
    let char_is_low = MsilMethodRef {
        owner: Some("[mscorlib]System.Char".to_string()),
        name: "IsLowSurrogate".to_string(),
        signature: MsilMethodSignature::new(MsilType::Bool, vec![MsilType::Char]),
    };
    let string_length = MsilMethodRef {
        owner: Some(string_owner()),
        name: "get_Length".to_string(),
        signature: MsilMethodSignature::new_instance(i32.clone(), Vec::new()),
    };
    let string_char_at = MsilMethodRef {
        owner: Some(string_owner()),
        name: "get_Chars".to_string(),
        signature: MsilMethodSignature::new_instance(MsilType::Char, vec![i32.clone()]),
    };
    MsilMethodBody {
        method: MsilMethodRef {
            owner: None,
            name: "__nyar_utf8_scalar_length".to_string(),
            signature: MsilMethodSignature::new(i32.clone(), vec![MsilType::String]),
        },
        // i, scalar_count, utf16_length
        locals: vec![i32.clone(), i32.clone(), i32.clone()],
        instructions: vec![
            MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Stloc0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Stloc1, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Callvirt, operand: Some(MsilInstructionOperand::Method(string_length)) },
            MsilInstruction { label: None, opcode: MsilOpcode::Stloc2, operand: None },
            MsilInstruction { label: Some("scalar_length_cond".to_string()), opcode: MsilOpcode::Nop, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldloc0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldloc2, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Blt,
                operand: Some(MsilInstructionOperand::BranchTarget("scalar_length_loop".to_string())),
            },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Br,
                operand: Some(MsilInstructionOperand::BranchTarget("scalar_length_done".to_string())),
            },
            MsilInstruction { label: Some("scalar_length_loop".to_string()), opcode: MsilOpcode::Nop, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldloc0, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Callvirt,
                operand: Some(MsilInstructionOperand::Method(string_char_at.clone())),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::Call, operand: Some(MsilInstructionOperand::Method(char_is_high)) },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Brfalse,
                operand: Some(MsilInstructionOperand::BranchTarget("scalar_length_count".to_string())),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldloc0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_1, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Add, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Stloc0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldloc0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldloc2, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Bge,
                operand: Some(MsilInstructionOperand::BranchTarget("scalar_length_count_after_advance".to_string())),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldloc0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Callvirt, operand: Some(MsilInstructionOperand::Method(string_char_at)) },
            MsilInstruction { label: None, opcode: MsilOpcode::Call, operand: Some(MsilInstructionOperand::Method(char_is_low)) },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Brfalse,
                operand: Some(MsilInstructionOperand::BranchTarget("scalar_length_count_after_advance".to_string())),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldloc0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_1, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Add, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Stloc0, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Br,
                operand: Some(MsilInstructionOperand::BranchTarget("scalar_length_count_after_advance".to_string())),
            },
            MsilInstruction { label: Some("scalar_length_count".to_string()), opcode: MsilOpcode::Nop, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldloc1, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_1, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Add, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Stloc1, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldloc0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_1, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Add, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Stloc0, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Br,
                operand: Some(MsilInstructionOperand::BranchTarget("scalar_length_cond".to_string())),
            },
            MsilInstruction { label: Some("scalar_length_count_after_advance".to_string()), opcode: MsilOpcode::Nop, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldloc1, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_1, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Add, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Stloc1, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Br,
                operand: Some(MsilInstructionOperand::BranchTarget("scalar_length_cond".to_string())),
            },
            MsilInstruction { label: Some("scalar_length_done".to_string()), opcode: MsilOpcode::Nop, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldloc1, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 4,
        is_entry_point: false,
        is_async: false,
    }
}

fn string_owner() -> String {
    "[mscorlib]System.String".to_string()
}

fn utf8_host_length_stub() -> MsilMethodBody {
    MsilMethodBody {
        method: MsilMethodRef {
            owner: None,
            name: "std_text___host_utf8_length".to_string(),
            signature: MsilMethodSignature::new(MsilType::Int32 { signed: true }, vec![MsilType::String]),
        },
        locals: Vec::new(),
        instructions: vec![
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Callvirt,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some(string_owner()),
                    name: "get_Length".to_string(),
                    signature: MsilMethodSignature::new_instance(MsilType::Int32 { signed: true }, Vec::new()),
                })),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

fn utf8_host_replace_stub() -> MsilMethodBody {
    MsilMethodBody {
        method: MsilMethodRef {
            owner: None,
            name: "std_text___host_utf8_replace".to_string(),
            signature: MsilMethodSignature::new(MsilType::String, vec![MsilType::String, MsilType::String, MsilType::String]),
        },
        locals: Vec::new(),
        instructions: vec![
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg1, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg2, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Callvirt,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some(string_owner()),
                    name: "Replace".to_string(),
                    signature: MsilMethodSignature::new_instance(MsilType::String, vec![MsilType::String, MsilType::String]),
                })),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

fn utf8_host_concat_stub() -> MsilMethodBody {
    MsilMethodBody {
        method: MsilMethodRef {
            owner: None,
            name: "std_text___host_utf8_concat".to_string(),
            signature: MsilMethodSignature::new(MsilType::String, vec![MsilType::String, MsilType::String]),
        },
        locals: Vec::new(),
        instructions: vec![
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg1, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Call,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some(string_owner()),
                    name: "Concat".to_string(),
                    signature: MsilMethodSignature::new(MsilType::String, vec![MsilType::String, MsilType::String]),
                })),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

fn utf8_host_slice_stub() -> MsilMethodBody {
    MsilMethodBody {
        method: MsilMethodRef {
            owner: None,
            name: "std_text___host_utf8_slice".to_string(),
            signature: MsilMethodSignature::new(
                MsilType::String,
                vec![MsilType::String, MsilType::Int32 { signed: true }, MsilType::Int32 { signed: true }],
            ),
        },
        locals: Vec::new(),
        instructions: vec![
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg1, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg2, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Callvirt,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some(string_owner()),
                    name: "Substring".to_string(),
                    signature: MsilMethodSignature::new_instance(
                        MsilType::String,
                        vec![MsilType::Int32 { signed: true }, MsilType::Int32 { signed: true }],
                    ),
                })),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

fn utf8_host_index_of_stub() -> MsilMethodBody {
    MsilMethodBody {
        method: MsilMethodRef {
            owner: None,
            name: "std_text___host_utf8_index_of".to_string(),
            signature: MsilMethodSignature::new(MsilType::Int32 { signed: true }, vec![MsilType::String, MsilType::String]),
        },
        locals: Vec::new(),
        instructions: vec![
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg1, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Callvirt,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some(string_owner()),
                    name: "IndexOf".to_string(),
                    signature: MsilMethodSignature::new_instance(MsilType::Int32 { signed: true }, vec![MsilType::String]),
                })),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

fn utf8_host_starts_with_stub() -> MsilMethodBody {
    utf8_host_binary_string_bool_stub("std_text___host_utf8_starts_with", "StartsWith")
}

fn utf8_host_ends_with_stub() -> MsilMethodBody {
    utf8_host_binary_string_bool_stub("std_text___host_utf8_ends_with", "EndsWith")
}

fn utf8_host_equals_stub() -> MsilMethodBody {
    utf8_host_binary_string_bool_stub("std_text___host_utf8_equals", "Equals")
}

fn utf8_host_binary_string_bool_stub(name: &str, clr_method: &str) -> MsilMethodBody {
    MsilMethodBody {
        method: MsilMethodRef {
            owner: None,
            name: name.to_string(),
            signature: MsilMethodSignature::new(MsilType::Bool, vec![MsilType::String, MsilType::String]),
        },
        locals: Vec::new(),
        instructions: vec![
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg1, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Callvirt,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some(string_owner()),
                    name: clr_method.to_string(),
                    signature: MsilMethodSignature::new_instance(MsilType::Bool, vec![MsilType::String]),
                })),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

fn ensure_runtime_stubs(global_methods: &mut Vec<MsilMethodBody>) {
    let defined: std::collections::BTreeSet<String> = global_methods.iter().map(|method| method.method.name.clone()).collect();
    let mut needed: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for method in global_methods.iter() {
        for instruction in &method.instructions {
            let Some(MsilInstructionOperand::Method(method_ref)) = instruction.operand.as_ref()
            else {
                continue;
            };
            if method_ref.owner.is_some() {
                continue;
            }
            let name = method_ref.name.as_str();
            if !(INJECTED_RUNTIME_STUBS.contains(&name) || is_tuple_get_stub_name(name)) {
                continue;
            }
            if defined.contains(&method_ref.name) {
                continue;
            }
            needed.insert(method_ref.name.clone());
        }
    }
    for name in needed {
        match name.as_str() {
            "panic" => global_methods.push(panic_runtime_stub()),
            "unimplemented" => global_methods.push(unimplemented_runtime_stub()),
            "is_null" => global_methods.push(is_null_runtime_stub()),
            "unwrap_null" => global_methods.push(unwrap_null_runtime_stub()),
            "print" => global_methods.push(print_runtime_stub()),
            "format" => global_methods.push(format_runtime_stub()),
            other if is_tuple_get_stub_name(other) => global_methods.push(tuple_get_runtime_stub(other)),
            _ => {}
        }
    }
}

/// Instance dispatch may emit `call Array_get` without the `Array.get` operation being in
/// the fragment's MIR export set. Inject the ordinal wrapper so `reject_unresolved_local_calls`
/// does not fail after a successful lower.
fn ensure_referenced_array_ordinal_helpers(global_methods: &mut Vec<MsilMethodBody>) {
    let defined: std::collections::BTreeSet<String> = global_methods.iter().map(|method| method.method.name.clone()).collect();
    let mut needed: std::collections::BTreeSet<(String, MsilMethodSignature)> = std::collections::BTreeSet::new();
    for method in global_methods.iter() {
        for instruction in &method.instructions {
            let Some(MsilInstructionOperand::Method(method_ref)) = instruction.operand.as_ref()
            else {
                continue;
            };
            if method_ref.owner.is_some() || defined.contains(&method_ref.name) {
                continue;
            }
            if matches!(method_ref.name.as_str(), "Array_get" | "Array_set") {
                needed.insert((method_ref.name.clone(), method_ref.signature.clone()));
            }
        }
    }
    for (name, signature) in needed {
        let kind = if name.ends_with("_set") { "set" } else { "get" };
        let operation = QualifiedName::new(vec![Identifier::new("Array"), Identifier::new(kind)]);
        let mut body = synthesize_array_ordinal_host_standalone(&operation, kind);
        // Preserve the call-site mangled name (`Array_get`) and observed signature arity.
        body.method.name = name;
        if !signature.parameter_types.is_empty() {
            body.method.signature.parameter_types = signature.parameter_types;
        }
        global_methods.push(body);
    }
}

/// Pattern/tuple extractors may Call bare `tuple_get_N` without a MethodDef.
/// Index 0 returns the object unchanged; higher indices return null until real
/// tuple layouts are wired through CLR FieldGet.
fn tuple_get_runtime_stub(name: &str) -> MsilMethodBody {
    let index = name.strip_prefix("tuple_get_").and_then(|suffix| suffix.parse::<u32>().ok()).unwrap_or(0);
    let instructions = if index == 0 {
        vec![
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ]
    }
    else {
        vec![
            MsilInstruction { label: None, opcode: MsilOpcode::Ldnull, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ]
    };
    MsilMethodBody {
        method: MsilMethodRef {
            owner: None,
            name: name.to_string(),
            signature: MsilMethodSignature::new(MsilType::Object, vec![MsilType::Object]),
        },
        locals: Vec::new(),
        instructions,
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

fn panic_runtime_stub() -> MsilMethodBody {
    MsilMethodBody {
        method: MsilMethodRef {
            owner: None,
            name: "panic".to_string(),
            signature: MsilMethodSignature::new(MsilType::Void, vec![MsilType::Object]),
        },
        locals: Vec::new(),
        instructions: vec![
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Call,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some("[System.Console]System.Console".to_string()),
                    name: "WriteLine".to_string(),
                    signature: MsilMethodSignature::new(MsilType::Void, vec![MsilType::Object]),
                })),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_1, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Call,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some("[mscorlib]System.Environment".to_string()),
                    name: "Exit".to_string(),
                    signature: MsilMethodSignature::new(MsilType::Void, vec![MsilType::Int32 { signed: true }]),
                })),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

/// `@unimplemented` → bare Call with no args; used as diverging match-arm expression.
fn unimplemented_runtime_stub() -> MsilMethodBody {
    MsilMethodBody {
        method: MsilMethodRef { owner: None, name: "unimplemented".to_string(), signature: MsilMethodSignature::new(MsilType::Object, vec![]) },
        locals: Vec::new(),
        instructions: vec![
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Ldstr,
                operand: Some(MsilInstructionOperand::StringLiteral("unimplemented".to_string())),
            },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Call,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some("[System.Console]System.Console".to_string()),
                    name: "WriteLine".to_string(),
                    signature: MsilMethodSignature::new(MsilType::Void, vec![MsilType::Object]),
                })),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_1, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Call,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some("[mscorlib]System.Environment".to_string()),
                    name: "Exit".to_string(),
                    signature: MsilMethodSignature::new(MsilType::Void, vec![MsilType::Int32 { signed: true }]),
                })),
            },
            // Unreachable after Exit; keeps the Object return signature PEVerify-legal.
            MsilInstruction { label: None, opcode: MsilOpcode::Ldnull, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

fn is_null_runtime_stub() -> MsilMethodBody {
    MsilMethodBody {
        method: MsilMethodRef {
            owner: None,
            name: "is_null".to_string(),
            signature: MsilMethodSignature::new(MsilType::Bool, vec![MsilType::Int64 { signed: true }]),
        },
        locals: Vec::new(),
        instructions: vec![
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::LdcI8, operand: Some(MsilInstructionOperand::Integer(i64::MIN as i64)) },
            MsilInstruction { label: None, opcode: MsilOpcode::Ceq, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

fn unwrap_null_runtime_stub() -> MsilMethodBody {
    MsilMethodBody {
        method: MsilMethodRef {
            owner: None,
            name: "unwrap_null".to_string(),
            signature: MsilMethodSignature::new(MsilType::Int64 { signed: true }, vec![MsilType::Int64 { signed: true }]),
        },
        locals: Vec::new(),
        instructions: vec![
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

/// `print(obj)` 运行时 stub：转发到 `System.Console.WriteLine(object)` 并返回 0。
///
/// 与 `panic` 对称，但 `print` 不调用 `Environment.Exit`，仅输出后正常返回。
/// 返回 `Int32 0` 以匹配 `witness_call_signature("print")` 推断的调用点签名。
fn print_runtime_stub() -> MsilMethodBody {
    MsilMethodBody {
        method: MsilMethodRef {
            owner: None,
            name: "print".to_string(),
            signature: MsilMethodSignature::new(MsilType::Int32 { signed: true }, vec![MsilType::Object]),
        },
        locals: Vec::new(),
        instructions: vec![
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Call,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some("[System.Console]System.Console".to_string()),
                    name: "WriteLine".to_string(),
                    signature: MsilMethodSignature::new(MsilType::Void, vec![MsilType::Object]),
                })),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

/// `format(fmt, value)` 运行时 stub：忽略模板，把 `value` 转成字符串。
///
/// 当前 guest 只用 `format("{}", x)`；完整模板解析可后续补。
fn format_runtime_stub() -> MsilMethodBody {
    MsilMethodBody {
        method: MsilMethodRef {
            owner: None,
            name: "format".to_string(),
            signature: MsilMethodSignature::new(MsilType::String, vec![MsilType::Object, MsilType::Object]),
        },
        locals: Vec::new(),
        instructions: vec![
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg1, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Call,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some("[mscorlib]System.Convert".to_string()),
                    name: "ToString".to_string(),
                    signature: MsilMethodSignature::new(MsilType::String, vec![MsilType::Object]),
                })),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

/// `von_parse_take_{fine|fail}(result)`：若 `result.tag == expected` 返回 `payload`，否则 `null`。
///
/// 对齐 CLR unite `Result` 布局（`tag` + `payload`）。`Fine=0`，`Fail=1`。
fn von_parse_take_variant_stub(name: &str, expected_tag: i32) -> MsilMethodBody {
    let result_type = "Result".to_string();
    MsilMethodBody {
        method: MsilMethodRef {
            owner: None,
            name: name.to_string(),
            signature: MsilMethodSignature::new(MsilType::Object, vec![MsilType::Object]),
        },
        locals: Vec::new(),
        instructions: vec![
            // if (result == null) return null;
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Brfalse,
                operand: Some(MsilInstructionOperand::BranchTarget("ret_null".to_string())),
            },
            // if (result.tag != expected) return null;
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Ldfld,
                operand: Some(MsilInstructionOperand::Field(result_type.clone(), "tag".to_string())),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::LdcI4, operand: Some(MsilInstructionOperand::Integer(expected_tag as i64)) },
            MsilInstruction { label: None, opcode: MsilOpcode::Ceq, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Brfalse,
                operand: Some(MsilInstructionOperand::BranchTarget("ret_null".to_string())),
            },
            // return result.payload;
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Ldfld,
                operand: Some(MsilInstructionOperand::Field(result_type, "payload".to_string())),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
            MsilInstruction { label: Some("ret_null".to_string()), opcode: MsilOpcode::Ldnull, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

fn is_direct_clr_import(submission: &FragmentSubmission, operation: &QualifiedName) -> bool {
    // `Array.get` / `Array.set` are ordinal host contracts. Even when
    // `resolve_host_contract_links` rebinds them to `GetValue`/`SetValue`, they must keep a
    // local MethodDef that performs ordinal→offset (never raw BCL at call sites).
    if is_array_ordinal_host_operation(operation).is_some() {
        return false;
    }
    submission.external_import_links.get(operation).and_then(clr_host_method_target).is_some()
}

/// `…Array.get` / `…Array.set` — ordinal host contracts (1-based), not cardinal offset FFI.
pub(crate) fn is_array_ordinal_host_operation(operation: &QualifiedName) -> Option<&'static str> {
    let parts = operation.parts();
    if parts.len() < 2 {
        return None;
    }
    if parts[parts.len() - 2].as_str() != "Array" {
        return None;
    }
    match parts.last().map(|part| part.as_str()) {
        Some("get") => Some("get"),
        Some("set") => Some("set"),
        _ => None,
    }
}

/// 拒绝所有增强完成后仍未解析的本地调用。
///
/// CLR 后端不得用返回默认值的桩掩盖缺失 MIR、witness 或运行时实现；否则产物
/// 虽能写出 PE，却没有执行源码语义，属于虚假自举。
pub(crate) fn reject_unresolved_local_calls(module: &MsilModule) -> Result<()> {
    let defined: std::collections::BTreeSet<String> = module.global_methods.iter().map(|method| method.method.name.clone()).collect();
    let mut unresolved: std::collections::BTreeMap<String, MsilMethodSignature> = std::collections::BTreeMap::new();
    collect_unresolved_calls(&module.global_methods, &defined, &mut unresolved);
    for type_def in &module.types {
        collect_unresolved_calls(&type_def.methods, &defined, &mut unresolved);
    }
    if unresolved.is_empty() {
        return Ok(());
    }
    let names = unresolved.keys().cloned().collect::<Vec<_>>().join(", ");
    let defined_sample = defined.iter().filter(|name| name.contains("infix")).take(20).cloned().collect::<Vec<_>>().join(", ");
    Err(miette!(
        code = "nyar::clr::unresolved_local_call",
        help = "请由前端提交对应 MIR，或由 witness/suspend/基础 CLR FFI 显式提供实现",
        "CLR 模块仍包含未解析的本地调用：{names}\n已定义的 infix 方法样例：{defined_sample}"
    ))
}

/// 扫描方法体列表，将 `owner: None` 且未定义的调用目标收集到 `unresolved`。
fn collect_unresolved_calls(
    methods: &[MsilMethodBody],
    defined: &std::collections::BTreeSet<String>,
    unresolved: &mut std::collections::BTreeMap<String, MsilMethodSignature>,
) {
    for method in methods {
        for instruction in &method.instructions {
            if instruction.opcode != MsilOpcode::Call {
                continue;
            }
            if let Some(MsilInstructionOperand::Method(ref method_ref)) = instruction.operand {
                if method_ref.owner.is_none() && !defined.contains(&method_ref.name) {
                    unresolved.entry(method_ref.name.clone()).or_insert_with(|| method_ref.signature.clone());
                }
            }
        }
    }
}

fn lower_operation_method(submission: &FragmentSubmission, operation: &QualifiedName) -> Result<MsilMethodBody> {
    if let Some(kind) = is_array_ordinal_host_operation(operation) {
        if let Some(executable) = submission.executable.as_ref() {
            if let Some(view) = executable.get_function(operation) {
                return lower_mir_function_to_msil(submission, operation, &view.function);
            }
        }
        // Externalized host_contract (link only): still emit ordinal wrapper MethodDef.
        return Ok(synthesize_array_ordinal_host_standalone(operation, kind));
    }
    let executable = submission.executable.as_ref().ok_or_else(|| {
        miette!(
            code = "nyar::clr::missing_mir",
            help = "CLR 本地函数必须由 Rust seed 前端提交可执行 MIR；外部能力只能声明为基础 `[clr(...)]` FFI",
            "CLR 无法降级本地操作 `{operation}`：FragmentSubmission 缺少 MIR provider"
        )
    })?;
    let view = executable.get_function(operation).ok_or_else(|| {
        let method_name = operation.parts().last().map(|part| part.as_str()).unwrap_or("");
        let candidates = executable
            .operations()
            .iter()
            .filter(|candidate| candidate.parts().last().is_some_and(|part| part.as_str() == method_name))
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let candidate_hint =
            if candidates.is_empty() { "无同名 MIR 候选".to_string() } else { format!("同名 MIR 候选：{}", candidates.join(", ")) };
        miette!(
            code = "nyar::clr::missing_mir_function",
            help = format!("请修复前端 MIR 可达闭包；CLR 后端不会为缺失实现生成返回 0、默认值或产品专用替代实现。{candidate_hint}"),
            "CLR 无法降级本地操作 `{operation}`：MIR provider 中不存在该函数"
        )
    })?;
    lower_mir_function_to_msil(submission, operation, &view.function)
}

fn lower_entry_instructions(submission: &FragmentSubmission, entry_operation: Option<&QualifiedName>) -> Vec<MsilInstruction> {
    if let Some(entry_operation) = entry_operation {
        return vec![
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Call,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: None,
                    name: sanitize_operation_symbol(entry_operation),
                    signature: method_signature_for(submission, entry_operation),
                })),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ];
    }

    vec![
        MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
    ]
}

fn collect_clr_externs(submission: &FragmentSubmission) -> Vec<String> {
    let mut externs = Vec::new();
    for link in submission.external_import_links.values() {
        let Some(target) = clr_host_method_target(link)
        else {
            continue;
        };

        for segment in [target.assembly, target.owner] {
            if !externs.iter().any(|existing| existing == segment) {
                externs.push(segment.to_string());
            }
        }
    }
    externs
}

fn expand_operations_with_mir_callees(submission: &FragmentSubmission, operations: &mut Vec<QualifiedName>) {
    let Some(exec) = submission.executable.as_ref()
    else {
        return;
    };
    let mut index = 0usize;
    while index < operations.len() {
        let operation = operations[index].clone();
        index += 1;
        let Some(view) = exec.get_function(&operation)
        else {
            continue;
        };
        for callee in collect_mir_callee_operations(exec.as_ref(), &view.function) {
            if !operations.iter().any(|existing| existing == &callee) {
                operations.push(callee);
            }
        }
    }
}

fn collect_mir_callee_operations(exec: &dyn ExecutableProvider, mir_fn: &MirFunction) -> Vec<QualifiedName> {
    let mut callees = Vec::new();
    for block in &mir_fn.blocks {
        for instruction in &block.instructions {
            let MirInstructionKind::Call { callee, arguments, .. } = &instruction.kind
            else {
                continue;
            };
            let MirOperand::Symbol(path) = callee
            else {
                continue;
            };
            if let Some(operation) = resolve_mir_callee_operation(exec, mir_fn, path, arguments) {
                callees.push(operation);
            }
            // 对于单段方法路径（如 `for_each`），`resolve_static_call_symbol`（mir.rs）
            // 会从全部操作中按末段名匹配并选择最佳候选。这里同步添加所有匹配操作，
            // 确保无论调用点最终选择哪个候选，对应的方法定义都已包含在模块中，
            // 避免 `MissingLocalMethodToken` 错误。
            if path.parts().len() == 1 {
                if let Some(method_name) = path.parts().last() {
                    for operation in exec.operations() {
                        if operation.parts().last().map(|part| part.as_str()) == Some(method_name.as_str()) {
                            callees.push(operation.clone());
                        }
                    }
                }
            }
        }
    }
    callees
}

fn resolve_mir_callee_operation(
    exec: &dyn ExecutableProvider,
    mir_fn: &MirFunction,
    path: &NamePath,
    arguments: &[MirOperand],
) -> Option<QualifiedName> {
    let method_name = path.parts().last()?.as_str();
    if path.parts().len() == 1 {
        // 优先用接收者类型精确匹配实例方法 owner。
        // 同时检查 `value_types` 和入口块参数类型（与 mir.rs 的 `lookup_value_type` 一致），
        // 覆盖 singleton 方法的 `self` 参数等场景。
        let receiver_ty = arguments.first().and_then(|argument| match argument {
            MirOperand::Value(value) => lookup_mir_value_type(mir_fn, value).cloned(),
            _ => None,
        });
        if let Some(receiver_ty) = receiver_ty {
            for operation in exec.operations() {
                if operation.parts().len() < 2 || operation.parts().last().map(|part| part.as_str()) != Some(method_name) {
                    continue;
                }
                let Some(owner_part) = operation.parts().get(operation.parts().len().saturating_sub(2))
                else {
                    continue;
                };
                if mir_receiver_type_name(&receiver_ty).is_some_and(|name| name == owner_part.as_str() || name.ends_with(owner_part.as_str())) {
                    return Some(operation.clone());
                }
            }
        }
        // No bare-name fallback. A caller without a qualified operation or
        // receiver type has not supplied enough Semantic MIR metadata; it
        // must remain unresolved and be rejected by the verifier rather than
        // selecting an arbitrary overload.
    }
    else if path.parts().len() > 1 {
        let qualified = QualifiedName::new(path.parts().to_vec());
        if exec.get_function(&qualified).is_some() {
            return Some(qualified);
        }
    }
    None
}

/// 查找 MIR 值引用的类型，优先查 `value_types`，回退到入口块参数对应的 `param_types`。
///
/// 入口块参数（如 singleton 方法中的 `self`）可能不在 `value_types` 中，
/// 但其类型可以从 `param_types` 按参数索引取到。
fn lookup_mir_value_type<'a>(mir_fn: &'a MirFunction, value: &MirValueRef) -> Option<&'a NyarType> {
    if let Some(ty) = mir_fn.value_types.get(value) {
        return Some(ty);
    }
    let entry_block = mir_fn.blocks.get(mir_fn.entry.0 as usize)?;
    for (index, param) in entry_block.parameters.iter().enumerate() {
        if param == value {
            return mir_fn.param_types.get(index);
        }
    }
    None
}

fn mir_receiver_type_name(ty: &NyarType) -> Option<&str> {
    match ty {
        NyarType::Utf8 => Some("utf8"),
        NyarType::Utf16 => Some("utf16"),
        NyarType::Array(_) => Some("Array"),
        NyarType::Named(name) => Some(name.as_str()),
        NyarType::Apply(base, _) => mir_receiver_type_name(base),
        _ => None,
    }
}
