use std::collections::BTreeMap;

use nyar::{ExternalCallArgument, ExternalCallEdge, InternalCallEdge, QualifiedName};
use nyar_types::IntrinsicId;
use nyar_bytecode::{
    NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarHeadCode, NyarImport, NyarImportKind, NyarModuleData, NYAR_VERSION,
};

use super::sanitize_symbol;
use crate::FragmentSubmission;

/// 宿主 builtin 导入模块名。
const HOST_IMPORT_MODULE: &str = "nyar.host";

struct BytecodeEmitter {
    constants: Vec<NyarConstant>,
    code_bytes: Vec<u8>,
    imports: Vec<NyarImport>,
}

impl BytecodeEmitter {
    fn new() -> Self {
        Self { constants: Vec::new(), code_bytes: Vec::new(), imports: Vec::new() }
    }

    fn intern_string(&mut self, value: &str) -> i32 {
        let index = self.constants.len() as i32;
        self.constants.push(NyarConstant::String(value.to_string()));
        index
    }

    fn emit_plain(&mut self, opcode: NyarHeadCode) {
        nyar_bytecode::emit_plain(&mut self.code_bytes, opcode);
    }

    fn emit_imm1(&mut self, opcode: NyarHeadCode, operand: i32) {
        nyar_bytecode::emit_imm1(&mut self.code_bytes, opcode, operand);
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

    fn emit_call_import(&mut self, symbol: &str, arg_count: i32) {
        let import_index = if let Some((index, _)) = self
            .imports
            .iter()
            .enumerate()
            .find(|(_, import)| import.module_name == HOST_IMPORT_MODULE && import.symbol_name == symbol)
        {
            index as i32
        }
        else {
            let index = self.imports.len() as i32;
            self.imports.push(NyarImport {
                kind: NyarImportKind::Function,
                module_name: HOST_IMPORT_MODULE.to_string(),
                symbol_name: symbol.to_string(),
            });
            index
        };
        nyar_bytecode::emit_imm2(&mut self.code_bytes, NyarHeadCode::CallImport, import_index, arg_count);
    }

    fn emit_call_intrinsic(&mut self, intrinsic: IntrinsicId, arg_count: i32) {
        nyar_bytecode::emit_imm2(
            &mut self.code_bytes,
            NyarHeadCode::CallIntrinsic,
            intrinsic.bytecode_index() as i32,
            arg_count,
        );
    }
}

/// Lower a non-suspend fragment into a `.nyar` module payload.
///
/// Dispatch ???? fragment ?? MIR ???`mir_functions` ?????????
/// `nyar_vm_mir` ?????lowering ????????????StructNew / TupleNew /
/// FixedArrayNew / AggregateCopy / FieldGet / FieldSet??????????
/// edge-based lowering ???? MIR ??????????nullable helper ????
/// call edge ????????
pub(crate) fn lower_fragment_to_nyar_module(submission: &FragmentSubmission) -> NyarModuleData {
    if submission.suspend_runtime.is_some() && submission.exported_operations.is_empty() {
        return empty_module(submission);
    }

    // ???????MIR-backed lowering??????????????? fallthrough ??
    // ??edge-based ????`_ => {}` ???????
    if submission.executable.as_ref().is_some_and(|exec| !exec.operations().is_empty()) {
        return super::nyar_vm_mir::lower_fragment_mir_to_nyar_module(submission);
    }

    // ?????edge-based lowering???? nullable helper / ?? call edge??
    // ????????????????????????MIR ????????
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

    let mut emitter = BytecodeEmitter::new();
    let mut functions = Vec::new();
    let mut symbol_to_index = BTreeMap::new();
    let mut export_short_names = Vec::new();

    for operation in &local_operations {
        let index = functions.len() as i32;
        symbol_to_index.insert(operation.clone(), index);
        let short_name = nyar_public_export_name(submission, operation);
        export_short_names.push((short_name, index));
    }

    for operation in &local_operations {
        let code_offset = emitter.code_bytes.len() as i32;
        let bool_profile: Option<i64> = None;
        let try_call: Option<(QualifiedName, i64)> = None;

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
        version: NYAR_VERSION,
        name: format!("{}__{}", sanitize_symbol(&submission.module_name), sanitize_symbol(submission.fragment_id.as_str())),
        constants: emitter.constants,
        functions,
        imports: emitter.imports,
        exports,
        witness_entries: Vec::new(),
        code_bytes: emitter.code_bytes,
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: Vec::new(),
    };
    super::singleton::augment_nyar_module_with_singletons(submission, &mut module, &std::collections::BTreeMap::new());
    module
}

fn empty_module(submission: &FragmentSubmission) -> NyarModuleData {
    NyarModuleData {
        version: NYAR_VERSION,
        name: format!("{}__{}", sanitize_symbol(&submission.module_name), sanitize_symbol(submission.fragment_id.as_str())),
        constants: Vec::new(),
        functions: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        witness_entries: Vec::new(),
        code_bytes: Vec::new(),
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: Vec::new(),
    }
}

pub(crate) fn operation_short_name(operation: &QualifiedName) -> String {
    operation.parts().last().map(|part| part.as_str().to_string()).unwrap_or_else(|| sanitize_symbol(&operation.to_string()))
}

/// Resolve the public `.nyar` export symbol for a stable operation.
pub(crate) fn nyar_public_export_name(submission: &FragmentSubmission, operation: &QualifiedName) -> String {
    if let Some(public_name) = submission.wasm_export_names.get(operation) {
        return public_name.clone();
    }
    let short = operation_short_name(operation);
    if let Some((_, public_name)) = submission.wasm_export_names.iter().find(|(key, _)| operation_short_name(key) == short) {
        return public_name.clone();
    }
    operation_short_name(operation)
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
        emitter.emit_call_import(&native_name, arg_count);
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
    emitter.emit_call_intrinsic(IntrinsicId::IsNull, 1);
    let early_exit_jump = emitter.emit_jump_if_true_placeholder();
    emitter.emit_call_intrinsic(IntrinsicId::UnwrapNull, 1);
    emitter.emit_const_i32(expected_value as i32);
    emitter.emit_i32_ne();
    let panic_jump = emitter.emit_jump_if_true_placeholder();
    emitter.patch_jump_offset(early_exit_jump, emitter.code_bytes.len());
    emitter.emit_return_void();
    emitter.patch_jump_offset(panic_jump, emitter.code_bytes.len());
    let panic_message = emitter.intern_string("nullable value");
    emitter.emit_const_from_pool(panic_message);
    emitter.emit_call_import("panic", 1);
    emitter.emit_return_void();
}
