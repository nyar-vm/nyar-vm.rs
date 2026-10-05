//! Wasm / WASI 目标的 executable 物理编码与 GC 布局。
//!
//! `mir/` 是尚未迁移的历史目录名，不是另一份语言 MIR。
//! 正式入口只能消费 Compiler 的函数体；宿主包装只属于产物层，
//! 不得由调用摘要生成可执行语义。`suspend` 保留显式参数驱动的底层控制流编码。

mod cabi;
mod gc;
pub(crate) mod mir;
mod sections;

pub(crate) use cabi::{
    CABI_HEAP_DEFAULT_BASE, CABI_HEAP_GLOBAL_INDEX, LINEAR_HEAP_MIN_BASE, align_up_u32, cabi_heap_base_after_data, cabi_heap_global_section,
    memory_min_pages_for_heap_base, wasm_cabi_realloc_bump_body,
};
pub(crate) use gc::{WASM_GC_ANYREF, wasm_gc_array_type, wasm_gc_field_type_byte, wasm_gc_struct_type};
pub(crate) use sections::{
    append_wasm_code_bodies, append_wasm_exports, append_wasm_function_decls, append_wasm_i32_globals, append_wasm_types, code_section_bytes,
    count_wasm_function_decls, count_wasm_function_imports, count_wasm_globals, count_wasm_types, data_section_bytes, decode_uleb128,
    encode_name, encode_sleb128_i32, encode_sleb128_i64, encode_uleb128, export_section_bytes, function_section_bytes, global_section_bytes,
    global_section_with_i32_inits, import_section_bytes, insert_wasm_section, memory_section_bytes, type_section_bytes, wasm_function_body,
    wasm_function_type,
};

use crate::{
    FragmentSubmission,
    backend_plan_views::ExecutableConstant,
    nyar_backend_wasi::{WasmPackageKind, WasiPreview, WasmBinaryModule, WasmSection},
};
use miette::{Result, miette};
use nyar::{HostProjectionBoundary, NyarType};

/// Lower a fragment for a wasm host boundary (default WASI Preview2 package train).
pub(crate) fn lower_fragment_to_wasm_module(
    submission: &FragmentSubmission,
    host_boundary: HostProjectionBoundary,
) -> Result<(WasmBinaryModule, Vec<(String, String)>)> {
    lower_fragment_to_wasm_module_for(submission, host_boundary, WasiPreview::Preview2, WasmPackageKind::Binary)
}

/// Lower a fragment for a wasm host boundary with an explicit WASI package train.
///
/// 只编码 Compiler 提交的 executable；调用边摘要与宿主包装不得替代函数体。
pub(crate) fn lower_fragment_to_wasm_module_for(
    submission: &FragmentSubmission,
    host_boundary: HostProjectionBoundary,
    wasi_preview: WasiPreview,
    wasm_package_kind: WasmPackageKind,
) -> Result<(WasmBinaryModule, Vec<(String, String)>)> {
    let boundary_entry_name = match host_boundary {
        HostProjectionBoundary::WasmJsGlue => "main",
        HostProjectionBoundary::WasiComponent => "_start",
        other => return Err(miette!("WASM host boundary is not supported: {other:?}")),
    };
    crate::lowering::features::semantic_mir_contract::validate_submission(submission).map_err(|error| {
        miette::miette!("semantic MIR contract failed [{}] {} at {}: {}", error.code, error.function, error.location, error.detail)
    })?;
    let physical_backend = match host_boundary {
        HostProjectionBoundary::WasmJsGlue => crate::lowering::features::physical_contract::PhysicalBackend::WasmJsGlue,
        HostProjectionBoundary::WasiComponent => crate::lowering::features::physical_contract::PhysicalBackend::WasiComponent,
        _ => crate::lowering::features::physical_contract::PhysicalBackend::WasmCore,
    };
    crate::lowering::features::physical_contract::validate_physical_submission(submission, physical_backend).map_err(|error| {
        miette::miette!("physical contract failed [{}] {} at {}: {}", error.code, error.function, error.location, error.detail)
    })?;
    validate_text_encoding_projection(submission, host_boundary)?;
    let has_executable = !submission.backend_plan.instances().is_empty();
    if !has_executable {
        return Err(miette!("WASM requires Compiler-owned executable functions; call-edge replay and empty entry synthesis are not valid inputs"));
    }
    let executable = &submission.backend_plan;
    for operation in submission.backend_plan.wasm_export_names().keys().chain(submission.backend_plan.entry_operation().iter()) {
        if executable.get_function(operation).is_none() {
            return Err(miette!("WASM callable `{operation}` has no Compiler-owned executable body"));
        }
    }
    if wasm_package_kind == WasmPackageKind::Library {
        if submission.backend_plan.wasm_export_names().is_empty() {
            return Err(miette::miette!("library wasm package requires at least one `[export]` symbol"));
        }
    }
    let export_name = if let Some(name) = submission.backend_plan.wasm_export_names().values().next() {
        name.as_str()
    }
    else {
        if submission.backend_plan.entry_operation().is_none() {
            return Err(miette!("WASM binary package requires a Compiler-resolved entry"));
        }
        boundary_entry_name
    };
    let (mut module, imports) = mir::lower_fragment_mir_to_wasm_module_for(submission, export_name, wasi_preview, wasm_package_kind)?;

    prepend_nyar_custom_sections(&mut module, submission);
    super::singleton::append_singleton_metadata_sections(&mut module, submission);
    super::singleton::augment_wasm_with_singleton_accessors(&mut module, submission);
    mir::augment_wasm_with_value_aggregate_metadata(&mut module, submission);
    Ok((module, imports))
}

/// The current JS-glue and WASI component ABIs have an explicit UTF-8 carrier
/// only.  Keep UTF-16 out of both paths until each has its own declared ABI
/// projection; an i32 handle, JS string, or canonical ABI string is not proof
/// of the language encoding it represents.
fn validate_text_encoding_projection(submission: &FragmentSubmission, host_boundary: HostProjectionBoundary) -> Result<()> {
    let executable = &submission.backend_plan;
    for operation in executable.instances() {
        let Some(view) = executable.get_function(&operation)
        else {
            continue;
        };
        let function = &view.function;
        let mut sites = vec![("return type".to_string(), &function.return_type)];
        sites.extend(function.param_types.iter().enumerate().map(|(index, ty)| (format!("parameter {index}"), ty)));
        sites.extend(function.value_types.iter().map(|(value, ty)| (format!("SSA value {}", value.0), ty)));
        for (site, ty) in sites {
            if type_contains_utf16(ty) {
                return Err(miette!(
                    "WASM pre-emission verifier: {} function `{}` {site} requires an explicit UTF-16 ABI contract; {:?} must not use the UTF-8 carrier",
                    boundary_name(host_boundary),
                    function.symbol,
                    host_boundary,
                ));
            }
        }
        for block in &function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                if let crate::contracts::InstructionKind::LoadConstant { constant: ExecutableConstant::Utf16(_), .. } = &instruction.kind {
                    return Err(miette!(
                        "WASM pre-emission verifier: {} function `{}` block {} instruction {index} contains a UTF-16 literal without an explicit UTF-16 ABI contract",
                        boundary_name(host_boundary),
                        function.symbol,
                        block.id.0,
                    ));
                }
            }
        }
    }
    Ok(())
}

fn type_contains_utf16(ty: &NyarType) -> bool {
    match ty {
        NyarType::Utf16 => true,
        NyarType::Apply(base, arguments) => type_contains_utf16(base) || arguments.iter().any(type_contains_utf16),
        NyarType::Function(function) => type_contains_utf16(&function.return_type) || function.params.iter().any(type_contains_utf16),
        NyarType::Tuple(elements) | NyarType::Union(elements) => elements.iter().any(type_contains_utf16),
        NyarType::Array(element) | NyarType::FixedArray { element, .. } => type_contains_utf16(element),
        NyarType::TraitObject(object) => object.type_arguments.iter().any(type_contains_utf16),
        _ => false,
    }
}

fn boundary_name(host_boundary: HostProjectionBoundary) -> &'static str {
    match host_boundary {
        HostProjectionBoundary::WasmJsGlue => "WasmJsGlue",
        HostProjectionBoundary::WasiComponent => "WasiComponent",
        _ => "WASM",
    }
}

fn prepend_nyar_custom_sections(module: &mut WasmBinaryModule, submission: &FragmentSubmission) {
    let merged_theory = submission.backend_plan.theory_bundle().merged();
    let mut customs = vec![
        ("nyar.module".to_string(), submission.backend_plan.module_name().as_bytes().to_vec()),
        ("nyar.fragment".to_string(), submission.backend_plan.fragment_id().as_str().as_bytes().to_vec()),
    ];
    for operation in submission.backend_plan.exported_operations() {
        customs.push(("nyar.export".to_string(), operation.to_string().into_bytes()));
    }
    for capability in submission.backend_plan.required_capabilities() {
        customs.push(("nyar.capability".to_string(), capability.as_str().to_string().into_bytes()));
    }
    if let Some(entry) = submission.backend_plan.entry_operation() {
        customs.push(("nyar.entry".to_string(), entry.to_string().into_bytes()));
    }
    customs.push(("nyar.theory.rules".to_string(), merged_theory.rules.len().to_string().into_bytes()));
    customs.push(("nyar.theory.equations".to_string(), merged_theory.equations.len().to_string().into_bytes()));

    let mut prepended = customs.into_iter().map(|(name, bytes)| WasmSection { id: 0, name: Some(name), bytes }).collect::<Vec<_>>();
    prepended.append(&mut module.sections);
    module.sections = prepended;
}

#[cfg(test)]
mod text_encoding_tests {
    use std::{collections::BTreeMap, sync::Arc};

    use crate::{
        FragmentSubmission,
        contracts::{Block, BlockRef, ExecutableFunction, Terminator},
    };
    use nyar::{Identifier, NyarType, QualifiedName};

    use super::{HostProjectionBoundary, validate_text_encoding_projection};

    fn utf16_submission() -> FragmentSubmission {
        let operation = QualifiedName::new(vec![Identifier::new("entry")]);
        let function = ExecutableFunction {
            return_layout: None,
            value_layouts: BTreeMap::new(),
            symbol: "entry".to_string(),
            return_type: NyarType::Unit,
            param_types: vec![NyarType::Utf16],
            value_types: BTreeMap::new(),
            entry: BlockRef(0),
            values: Vec::new(),
            suspend_points: Vec::new(),
            frame_layouts: Vec::new(),
            continuations: Vec::new(),
            case_chains: Vec::new(),
            #[allow(deprecated)]
            state_machine: None,
            suspend_plan: None,
            blocks: vec![Block {
                id: BlockRef(0),
                label: "entry".to_string(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Terminator::Return { value: None },
            }],
            diagnostics: Vec::new(),
        };
        let mut submission = FragmentSubmission::default();
        submission.backend_plan = Arc::new(crate::BackendPrivatePlan::from_functions(BTreeMap::from([(operation, function)])));
        submission
    }

    #[test]
    fn rejects_utf16_without_a_declared_wasm_carrier_on_every_managed_boundary() {
        for boundary in [HostProjectionBoundary::WasmJsGlue, HostProjectionBoundary::WasiComponent] {
            let error = validate_text_encoding_projection(&utf16_submission(), boundary).expect_err("UTF-16 requires a declared carrier");
            let message = error.to_string();
            assert!(message.contains("WASM pre-emission verifier"), "{message}");
            assert!(message.contains("explicit UTF-16 ABI contract"), "{message}");
        }
    }
}
