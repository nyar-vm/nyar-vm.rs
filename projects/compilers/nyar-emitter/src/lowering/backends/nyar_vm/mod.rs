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

    if !submission.backend_plan.operations().is_empty() {
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

/// 仅按完整操作身份读取公开导出名；未指定公开名时保留完整身份。
pub(crate) fn nyar_public_export_name(submission: &FragmentSubmission, operation: &QualifiedName) -> String {
    if let Some(public_name) = submission.wasm_export_names.get(operation) {
        return public_name.clone();
    }
    operation.to_string()
}

#[cfg(test)]
mod export_identity_tests {
    use super::*;

    #[test]
    fn export_identity_does_not_borrow_another_owners_public_name() {
        let first = QualifiedName::new(vec![nyar::Identifier::new("first"), nyar::Identifier::new("method")]);
        let second = QualifiedName::new(vec![nyar::Identifier::new("second"), nyar::Identifier::new("method")]);
        let submission = FragmentSubmission {
            wasm_export_names: std::collections::BTreeMap::from([(first.clone(), "public_method".to_owned())]),
            ..Default::default()
        };
        assert_eq!(nyar_public_export_name(&submission, &first), "public_method");
        assert_eq!(nyar_public_export_name(&submission, &second), second.to_string());
    }
}

