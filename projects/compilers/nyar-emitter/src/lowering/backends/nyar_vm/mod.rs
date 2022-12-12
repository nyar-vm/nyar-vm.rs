use std::collections::BTreeMap;

use nyar::{ExternalCallArgument, ExternalCallEdge, InternalCallEdge, QualifiedName};
use std_data::binary::nyar_ir::{NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarHeadCode, NyarModuleData};

use super::sanitize_symbol;
use crate::FragmentSubmission;

struct BytecodeEmitter {
    constants: Vec<NyarConstant>,
    code_bytes: Vec<u8>,
}

impl BytecodeEmitter {
    fn new() -> Self {
        Self { constants: Vec::new(), code_bytes: Vec::new() }
    }

    fn intern_string(&mut self, value: &str) -> i32 {
        let index = self.constants.len() as i32;
        self.constants.push(NyarConstant::String(value.to_string()));
        index
    }

    fn emit_plain(&mut self, opcode: NyarHeadCode) {
        self.code_bytes.push(opcode as u8);
    }

    fn emit_imm1(&mut self, opcode: NyarHeadCode, operand: i32) {
        self.code_bytes.push(opcode as u8);
        self.code_bytes.extend_from_slice(&operand.to_le_bytes());
    }

    fn emit_return_void(&mut self) {
        self.emit_plain(NyarHeadCode::Return);
    }

    fn emit_const_i32(&mut self, value: i32) {
        let index = self.constants.len() as i32;
        self.constants.push(NyarConstant::Integer32(value));
        self.emit_imm1(NyarHeadCode::Const, index);
    }

    fn emit_const_bool(&mut self, value: bool) {
        let index = self.constants.len() as i32;
        self.constants.push(NyarConstant::Boolean(value));
        self.emit_imm1(NyarHeadCode::Const, index);
    }

    fn emit_const_null(&mut self) {
        let index = self.constants.len() as i32;
        self.constants.push(NyarConstant::Null);
        self.emit_imm1(NyarHeadCode::Const, index);
    }

    fn emit_const_from_pool(&mut self, index: i32) {
        self.emit_imm1(NyarHeadCode::Const, index);
    }

    fn emit_load_arg(&mut self, index: i32) {
        self.emit_imm1(NyarHeadCode::LoadArg, index);
    }

    fn emit_jump_if_true_placeholder(&mut self) -> usize {
        let position = self.code_bytes.len();
        self.emit_imm1(NyarHeadCode::JumpIfTrue, 0);
        position
    }

    fn emit_jump_if_false_placeholder(&mut self) -> usize {
        let position = self.code_bytes.len();
        self.emit_imm1(NyarHeadCode::JumpIfFalse, 0);
        position
    }

    fn patch_jump_offset(&mut self, jump_position: usize, target: usize) {
        let offset = (target as i32) - (jump_position as i32);
        self.code_bytes[jump_position + 1..jump_position + 5].copy_from_slice(&offset.to_le_bytes());
    }

    fn emit_i32_ne(&mut self) {
        self.emit_plain(NyarHeadCode::I32Ne);
    }

    fn emit_call(&mut self, function_index: i32) {
        self.emit_imm1(NyarHeadCode::Call, function_index);
    }

    fn emit_pop(&mut self) {
        self.emit_plain(NyarHeadCode::Pop);
    }

    fn emit_call_native(&mut self, name: &str, arg_count: i32) {
        let name_index = self.intern_string(name);
        self.code_bytes.push(NyarHeadCode::CallNative as u8);
        self.code_bytes.extend_from_slice(&name_index.to_le_bytes());
        self.code_bytes.extend_from_slice(&arg_count.to_le_bytes());
    }
}

/// Lower a non-suspend fragment into a `.nyar` module payload.
///
/// Dispatch 策略：当 fragment 携带 MIR 函数（`mir_functions` 非空）时，统一走
/// `nyar_vm_mir` 的值语义 lowering 主路径——这是值聚合体（StructNew / TupleNew /
/// FixedArrayNew / AggregateCopy / FieldGet / FieldSet）唯一可用的路径。旧
/// edge-based lowering 仅作为无 MIR 时的回退保留，负责 nullable helper 与外部
/// call edge 的字节码生成。
pub(crate) fn lower_fragment_to_nyar_module(submission: &FragmentSubmission) -> NyarModuleData {
    if submission.suspend_runtime.is_some() && submission.exported_operations.is_empty() {
        return empty_module(submission);
    }

    // 值语义主路径：MIR-backed lowering。值聚合体必须经此路径，否则会 fallthrough 到
    // 旧 edge-based 路径的 `_ => {}` 而丢失指令。
    if submission.executable.as_ref().is_some_and(|exec| !exec.operations().is_empty()) {
        return super::nyar_vm_mir::lower_fragment_mir_to_nyar_module(submission);
    }

    // 回退路径：edge-based lowering。仅处理 nullable helper / 外部 call edge，
    // 不支持值聚合体指令。值语义代码必须先在前端生成 MIR 再进入此函数。
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
    }
    for operation in submission.operation_literal_returns.keys() {
        if !local_operations.iter().any(|existing| existing == operation) {
            local_operations.push(operation.clone());
        }
    }

    let mut emitter = Bytecodenyar_emitter::new();
    let mut functions = Vec::new();
    let mut symbol_to_index = BTreeMap::new();
    let mut export_short_names = Vec::new();

    for operation in &local_operations {
        let index = functions.len() as i32;
        symbol_to_index.insert(operation.clone(), index);
        let short_name = operation_short_name(operation);
        export_short_names.push((short_name, index));
    }

    for operation in &local_operations {
        let code_offset = emitter.code_bytes.len() as i32;
        let bool_profile =
            submission.nullable_bool_profiles.iter().find(|profile| &profile.function == operation).map(|profile| profile.true_value);
        let try_call =
            submission.nullable_try_calls.iter().find(|call| &call.caller == operation && call.callee_arg_is_true).and_then(|call| {
                submission
                    .nullable_bool_profiles
                    .iter()
                    .find(|profile| profile.function == call.callee)
                    .map(|profile| (call.callee.clone(), profile.true_value))
            });

        let emitted_specialized = if let Some(true_value) = bool_profile {
            emit_bool_nullable_helper(&mut emitter, true_value);
            true
        }
        else if let Some((callee, expected_value)) = try_call {
            emit_nullable_try_propagate_test(&mut emitter, &callee, expected_value, &symbol_to_index);
            true
        }
        else {
            false
        };

        if !emitted_specialized {
            let external_edges = outgoing_external_call_edges(operation, &submission.external_call_edges);
            let internal_edges = outgoing_internal_call_edges(operation, &submission.internal_call_edges);
            let literal_return = submission.operation_literal_returns.get(operation);
            let returns_void = operation_returns_void(submission, operation);

            lower_operation_bytecode(
                &mut emitter,
                external_edges,
                internal_edges,
                literal_return,
                &submission.external_import_links,
                &symbol_to_index,
                &submission.operation_void_returns,
                returns_void,
            );
        }

        let short_name = operation_short_name(operation);
        let (arity, local_count) = if bool_profile.is_some() { (1, 1) } else { (0, 0) };
        functions.push(NyarFunction {
            name: short_name,
            arity,
            local_count,
            code_offset,
            code_length: emitter.code_bytes.len() as i32 - code_offset,
        });
    }

    let exports = export_short_names
        .into_iter()
        .map(|(symbol_name, function_index)| NyarExport { kind: NyarExportKind::Function, symbol_name, function_index })
        .collect();

    let mut module = NyarModuleData {
        version: 1,
        name: format!("{}__{}", sanitize_symbol(&submission.module_name), sanitize_symbol(submission.fragment_id.as_str())),
        constants: emitter.constants,
        functions,
        imports: Vec::new(),
        exports,
        witness_entries: Vec::new(),
        code_bytes: emitter.code_bytes,
        globals: Vec::new(),
        init_function_indices: Vec::new(),
    };
    super::singleton::augment_nyar_module_with_singletons(submission, &mut module, &std::collections::BTreeMap::new());
    module
}

fn empty_module(submission: &FragmentSubmission) -> NyarModuleData {
    NyarModuleData {
        version: 1,
        name: format!("{}__{}", sanitize_symbol(&submission.module_name), sanitize_symbol(submission.fragment_id.as_str())),
        constants: Vec::new(),
        functions: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        witness_entries: Vec::new(),
        code_bytes: Vec::new(),
        globals: Vec::new(),
        init_function_indices: Vec::new(),
    }
}

pub(crate) fn operation_short_name(operation: &QualifiedName) -> String {
    operation.parts().last().map(|part| part.as_str().to_string()).unwrap_or_else(|| sanitize_symbol(&operation.to_string()))
}

fn operation_returns_void(submission: &FragmentSubmission, operation: &QualifiedName) -> bool {
    submission.operation_void_returns.contains(operation)
}

fn lower_operation_bytecode(
    emitter: &mut BytecodeEmitter,
    external_call_edges: Vec<&ExternalCallEdge>,
    internal_call_edges: Vec<&InternalCallEdge>,
    literal_return: Option<&String>,
    external_import_links: &BTreeMap<QualifiedName, nyar::ExternalImportLink>,
    symbol_to_index: &BTreeMap<QualifiedName, i32>,
    void_returns: &std::collections::BTreeSet<QualifiedName>,
    returns_void: bool,
) {
    for edge in external_call_edges {
        let native_name = native_name_for_external_call(external_import_links.get(&edge.callee_symbol), &edge.arguments);
        let arg_count = edge.arguments.len() as i32;
        emitter.emit_call_native(&native_name, arg_count);
    }
    for edge in internal_call_edges {
        if let Some(&index) = symbol_to_index.get(&edge.callee_symbol) {
            emitter.emit_call(index);
            if !void_returns.contains(&edge.callee_symbol) {
                emitter.emit_pop();
            }
        }
    }
    if let Some(literal) = literal_return {
        let _ = emitter.intern_string(literal);
        emitter.emit_const_i32(0);
    }
    else if !returns_void {
        emitter.emit_const_i32(0);
    }
    emitter.emit_return_void();
}

fn native_name_for_external_call(external_import_link: Option<&nyar::ExternalImportLink>, arguments: &[ExternalCallArgument]) -> String {
    if let Some(link) = external_import_link {
        if let [.., method] = link.locator_segments.as_slice() {
            return method.clone();
        }
    }
    if arguments.iter().any(|argument| matches!(argument, ExternalCallArgument::StringLiteral(_))) {
        return "panic".to_string();
    }
    "console_log".to_string()
}

fn outgoing_external_call_edges<'a>(operation: &QualifiedName, edges: &'a [ExternalCallEdge]) -> Vec<&'a ExternalCallEdge> {
    edges.iter().filter(|edge| &edge.caller == operation).collect()
}

fn outgoing_internal_call_edges<'a>(operation: &QualifiedName, edges: &'a [InternalCallEdge]) -> Vec<&'a InternalCallEdge> {
    edges.iter().filter(|edge| &edge.caller == operation).collect()
}

fn emit_bool_nullable_helper(emitter: &mut BytecodeEmitter, true_value: i64) {
    emitter.emit_load_arg(0);
    let jump_to_null = emitter.emit_jump_if_false_placeholder();
    emitter.emit_const_i32(true_value as i32);
    emitter.emit_return_void();
    emitter.patch_jump_offset(jump_to_null, emitter.code_bytes.len());
    emitter.emit_const_null();
    emitter.emit_return_void();
}

fn emit_nullable_try_propagate_test(
    emitter: &mut BytecodeEmitter,
    callee: &QualifiedName,
    expected_value: i64,
    symbol_to_index: &BTreeMap<QualifiedName, i32>,
) {
    let Some(&callee_index) = symbol_to_index.get(callee)
    else {
        emitter.emit_return_void();
        return;
    };

    emitter.emit_const_bool(true);
    emitter.emit_call(callee_index);
    emitter.emit_call_native("is_null", 1);
    let early_exit_jump = emitter.emit_jump_if_true_placeholder();
    emitter.emit_call_native("unwrap_null", 1);
    emitter.emit_const_i32(expected_value as i32);
    emitter.emit_i32_ne();
    let panic_jump = emitter.emit_jump_if_true_placeholder();
    emitter.patch_jump_offset(early_exit_jump, emitter.code_bytes.len());
    emitter.emit_return_void();
    emitter.patch_jump_offset(panic_jump, emitter.code_bytes.len());
    let panic_message = emitter.intern_string("nullable value");
    emitter.emit_const_from_pool(panic_message);
    emitter.emit_call_native("panic", 1);
    emitter.emit_return_void();
}
