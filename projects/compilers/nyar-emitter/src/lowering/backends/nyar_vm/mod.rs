use nyar_bytecode::{NyarModuleData, NYAR_VERSION};

use super::sanitize_symbol;
use crate::FragmentSubmission;

/// 只发射 Compiler 实例键控的目标私有计划，不重放调用边。
pub(crate) fn lower_fragment_to_nyar_module(submission: &FragmentSubmission) -> miette::Result<NyarModuleData> {
    if submission.suspend_runtime.is_some() && submission.exported_operations.is_empty() {
        return Ok(empty_module(submission));
    }

    if !submission.backend_plan.instances().is_empty() {
        return super::nyar_vm_mir::lower_fragment_mir_to_nyar_module(submission);
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

