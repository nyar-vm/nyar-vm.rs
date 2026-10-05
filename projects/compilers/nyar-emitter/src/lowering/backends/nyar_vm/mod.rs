use nyar_bytecode::{NYAR_VERSION, NyarModuleData};

use super::sanitize_symbol;
use crate::FragmentSubmission;

/// 只发射 Compiler 实例键控的目标私有计划，不重放调用边。
pub(crate) fn lower_fragment_to_nyar_module(submission: &FragmentSubmission) -> miette::Result<NyarModuleData> {
    if !submission.backend_plan.instances().is_empty() {
        return super::nyar_vm_mir::lower_fragment_mir_to_nyar_module(submission);
    }
    Err(miette::miette!("Nyar VM backend requires Compiler-owned executable functions; edge-based semantic replay is not a valid input"))
}
