//! Wasm / WASI backend: Semantic MIR → Wasm physical module (GC required), plus host shells.
//!
//! Do **not** call this layer “Wasm MIR”. MIR is a Valkyrie frontend concept.
//! Backend ownership is prepare/encode toward a `WasmModuleModel` (physical module
//! structure), not another language IR.
//!
//! Layout (directory `mir/` is a **historical path name** only):
//! - [`mir`] — Semantic MIR → Wasm prepare/emit facade
//! - [`mir::control`] — CFG → Wasm structured control (`pc_local` / `loop` / `br_table`)
//! - [`mir::representation`] — language type/layout → unique Wasm physical storage/value type
//! - [`mir::type_registry`] — GC struct/array/sum type-index registration
//! - [`mir::calls`] — resolved calls → Wasm call forms
//! - [`cabi`] — shared linear bump heap (`cabi_realloc` + alloc)
//! - [`gc`] — struct/array type builders
//! - [`sections`] — binary section codecs
//! - [`suspend`] — CPS suspend/witness shells
//! - [`host`] — WasmJsGlue string interop + WasiComponent packaging shells

mod cabi;
mod gc;
mod host;
mod host_imports;
pub(crate) mod mir;
mod sections;
mod suspend;

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
pub(crate) use suspend::suspend_run_loop_with_witness_wasm_bytes;

use crate::{
    FragmentSubmission,
    artifacts::suspend_sidecar::serialize_control_flow_payload,
    executable_provider::ExecutableConstant,
    nyar_backend_wasi::{WasiPreview, WasmBinaryModule, WasmSection},
};
use miette::{Result, miette};
use nyar::{HostProjectionBoundary, NyarType};

/// Lower a fragment for a wasm host boundary (default WASI Preview2 package train).
pub(crate) fn lower_fragment_to_wasm_module(
    submission: &FragmentSubmission,
    host_boundary: HostProjectionBoundary,
) -> Result<(WasmBinaryModule, Vec<(String, String)>)> {
    lower_fragment_to_wasm_module_for(submission, host_boundary, WasiPreview::Preview2)
}

/// Lower a fragment for a wasm host boundary with an explicit WASI package train.
///
/// Ideal path: executable MIR + no witness → [`mir`] with mandatory GC.
/// Otherwise: host shell ([`host`]) for packaging / suspend / string interop.
pub(crate) fn lower_fragment_to_wasm_module_for(
    submission: &FragmentSubmission,
    host_boundary: HostProjectionBoundary,
    wasi_preview: WasiPreview,
) -> Result<(WasmBinaryModule, Vec<(String, String)>)> {
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
    let has_executable = submission.executable.as_ref().is_some_and(|exec| !exec.operations().is_empty());
    // Node string-output host imports (`emit_byte`) still require the js_glue shell until MIR
    // collects those imports. This is a host-interop gap, not a GC escape hatch.
    let needs_js_host_string_interop = match host_boundary {
        HostProjectionBoundary::WasmJsGlue => !host_imports::collect_string_output_for_wasm_host(submission).is_empty(),
        _ => false,
    };
    let use_mir_path = has_executable && submission.witness_calls.is_empty() && !needs_js_host_string_interop;
    let (mut module, imports) = if use_mir_path {
        let export_name = match host_boundary {
            HostProjectionBoundary::WasmJsGlue => "main",
            HostProjectionBoundary::WasiComponent => "_start",
            other => unreachable!("unexpected wasm host boundary: {:?}", other),
        };
        mir::lower_fragment_mir_to_wasm_module_for(submission, export_name, wasi_preview)
    }
    else {
        match host_boundary {
            HostProjectionBoundary::WasmJsGlue => host::lower_fragment_to_js_glue_module(submission),
            HostProjectionBoundary::WasiComponent => host::lower_fragment_to_wasi_cm_module_for(submission, wasi_preview),
            other => unreachable!("unexpected wasm host boundary: {:?}", other),
        }
    };

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
    let Some(executable) = &submission.executable
    else {
        return Ok(());
    };
    for operation in executable.operations() {
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
    let merged_theory = submission.theory_bundle.merged();
    let mut customs = vec![
        ("nyar.module".to_string(), submission.module_name.as_bytes().to_vec()),
        ("nyar.fragment".to_string(), submission.fragment_id.as_str().as_bytes().to_vec()),
    ];
    for operation in &submission.exported_operations {
        customs.push(("nyar.export".to_string(), operation.to_string().into_bytes()));
    }
    for capability in &submission.required_capabilities {
        customs.push(("nyar.capability".to_string(), capability.as_str().to_string().into_bytes()));
    }
    if let Some(entry) = &submission.entry_operation {
        customs.push(("nyar.entry".to_string(), entry.to_string().into_bytes()));
    }
    if let Some(payload) = &submission.control_flow {
        customs.push(("nyar.control_flow".to_string(), serialize_control_flow_payload(payload).into_bytes()));
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
        executable_provider::MirFunctionMapProvider,
    };
    use nyar::{Identifier, NyarType, QualifiedName};

    use super::{HostProjectionBoundary, validate_text_encoding_projection};

    fn utf16_submission() -> FragmentSubmission {
        let operation = QualifiedName::new(vec![Identifier::new("entry")]);
        let function = ExecutableFunction {
            symbol: "entry".to_string(),
            return_type: NyarType::Unit,
            param_types: vec![NyarType::Utf16],
            value_types: BTreeMap::new(),
            entry: BlockRef(0),
            values: Vec::new(),
            intrinsic: None,
            suspend_points: Vec::new(),
            frame_layouts: Vec::new(),
            continuations: Vec::new(),
            case_chains: Vec::new(),
            #[allow(deprecated)]
            state_machine: None,
            suspend_plan: None,
            state_machine_lowered: true,
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
        submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)]))));
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
