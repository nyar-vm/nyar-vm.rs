use nyar::QualifiedName;
use nyar_bytecode::{NyarModuleData, NYAR_VERSION};

use super::sanitize_symbol;
use crate::FragmentSubmission;

/// 宿主 builtin 导入模块名。
/// Lower a non-suspend fragment into a `.nyar` module payload.
///
/// Dispatch ???? fragment ?? MIR ???`mir_functions` ?????????
/// `nyar_vm_mir` ?????lowering ????????????StructNew / TupleNew /
/// FixedArrayNew / AggregateCopy / FieldGet / FieldSet??????????
/// edge-based lowering ???? MIR ??????????nullable helper ????
/// call edge ????????
pub(crate) fn lower_fragment_to_nyar_module(submission: &FragmentSubmission) -> miette::Result<NyarModuleData> {
    if submission.suspend_runtime.is_some() && submission.exported_operations.is_empty() {
        return Ok(empty_module(submission));
    }

    if submission.executable.as_ref().is_some_and(|exec| !exec.operations().is_empty()) {
        return Ok(super::nyar_vm_mir::lower_fragment_mir_to_nyar_module(submission));
    }
    Err(miette::miette!("Nyar VM backend requires Compiler-owned executable functions; edge-based semantic replay is not a valid input"))
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

