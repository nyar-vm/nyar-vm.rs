use std::path::{Path, PathBuf};

use crate::nyar_backend_wasi::{WasmBinaryBackendInput, WasmBinaryModule};
use miette::{IntoDiagnostic, Result, WrapErr, miette};
use nyar::{HostProjectionBoundary, TargetBackendFamily};
#[cfg(feature = "nyar-vm-lane")]
use nyar_bytecode::NyarModuleData;

use crate::{DriverBackendInput, FragmentSubmission};
#[cfg(feature = "nyar-vm-lane")]
use crate::NyarVmBackendInput;

pub(crate) mod backends;
pub(crate) mod features;
mod shared;
pub(crate) use self::features::pattern_matching_contract;

#[cfg(not(feature = "nyar-vm-lane"))]
use self::backends::wasm;
#[cfg(feature = "nyar-vm-lane")]
use self::backends::{nyar_vm, wasm};




pub(crate) fn testing_lower_fragment_to_wasm_mir_module(submission: &FragmentSubmission, export_name: &str) -> WasmBinaryModule {
    backends::wasm::mir::lower_fragment_mir_to_wasm_module(submission, export_name).0
}



#[cfg(feature = "nyar-vm-lane")]
pub(crate) fn testing_lower_fragment_to_nyar_module(submission: &FragmentSubmission) -> Result<NyarModuleData> {
    nyar_vm::lower_fragment_to_nyar_module(submission)
}




pub(crate) fn testing_decode_wasm_uleb128(bytes: &[u8], pos: &mut usize) -> u32 {
    wasm::decode_uleb128(bytes, pos)
}




pub(crate) fn testing_dispatch_case_keys(artifact: &nyar::SuspendFunctionArtifact) -> Vec<u32> {
    shared::suspend_sm::dispatch_case_keys(artifact)
}

pub(crate) fn testing_lower_fragment_to_wasm_module(
    submission: &FragmentSubmission,
    host_boundary: HostProjectionBoundary,
) -> Result<(WasmBinaryModule, Vec<(String, String)>)> {
    wasm::lower_fragment_to_wasm_module(submission, host_boundary)
}

pub(crate) fn testing_suspend_run_loop_with_witness_wasm_bytes(
    artifact: &nyar::SuspendFunctionArtifact,
    witness_offset: u32,
    witness_type_index: u32,
    method_index: u32,
    function_index: u32,
    returns_i32: bool,
) -> Vec<u8> {
    wasm::suspend_run_loop_with_witness_wasm_bytes(artifact, witness_offset, witness_type_index, method_index, function_index, returns_i32)
}







pub(crate) fn testing_mir_lowering_context(submission: &FragmentSubmission) -> shared::executable::ExecutableLoweringContext<'_> {
    shared::executable::ExecutableLoweringContext::new(submission)
}


pub(crate) const TESTING_WASM_GC_ANYREF: u8 = wasm::WASM_GC_ANYREF;

pub(crate) fn testing_append_singleton_metadata_sections(module: &mut WasmBinaryModule, submission: &FragmentSubmission) {
    features::singleton::append_singleton_metadata_sections(module, submission);
}

pub(crate) fn testing_augment_wasm_with_singleton_accessors(module: &mut WasmBinaryModule, submission: &FragmentSubmission) {
    features::singleton::augment_wasm_with_singleton_accessors(module, submission);
}

pub(crate) fn testing_singleton_metadata_line(plan: &nyar_types::SingletonInstancePlan) -> String {
    features::singleton::singleton_metadata_line(plan)
}

pub(crate) fn lower_fragment_to_driver_input(
    submission: &FragmentSubmission,
    backend_family: TargetBackendFamily,
    host_boundary: HostProjectionBoundary,
    output_dir: PathBuf,
    host_flavor: &str,
    wasm_package_kind: crate::nyar_backend_wasi::WasmPackageKind,
) -> Result<DriverBackendInput> {
    features::semantic_mir_contract::validate_submission(submission)
        .map_err(|error| miette!("semantic MIR contract failed [{}] {} at {}: {}", error.code, error.function, error.location, error.detail))?;
    match backend_family {
        TargetBackendFamily::Wasm => {
            let wasi_preview = crate::nyar_backend_wasi::WasiPreview::from_host_flavor(host_flavor);
            let (module, imports) = wasm::lower_fragment_to_wasm_module_for(submission, host_boundary, wasi_preview, wasm_package_kind)?;
            // WASI component model: `_start` is always nullary (argv is read via
            // `wasi:cli/environment.get-arguments`), so any WASI partition with an
            // executable entry should be packaged as a command component.
            let has_executable_entry = !submission.backend_plan.instances().is_empty();
            let package_as_wasi_command = host_boundary == HostProjectionBoundary::WasiComponent && has_executable_entry;
            Ok(DriverBackendInput::Wasm(WasmBinaryBackendInput {
                module,
                output_dir,
                host_boundary,
                imports,
                control_flow: submission.control_flow.clone(),
                package_as_wasi_command,
                wasi_preview,
                library_wasm_exports: submission.wasm_export_names.values().cloned().collect(),
                wasm_package_kind,
            }))
        }
        #[cfg(feature = "nyar-vm-lane")]
        TargetBackendFamily::NyarVm => {
            let nyar_module = if submission.suspend_runtime.is_some() && submission.exported_operations.is_empty() {
                None
            }
            else {
                Some(nyar_vm::lower_fragment_to_nyar_module(submission)?)
            };
            Ok(DriverBackendInput::NyarVm(NyarVmBackendInput {
                suspend_runtime: submission.suspend_runtime.clone(),
                control_flow: submission.control_flow.clone(),
                nyar_module,
                library_public_exports: submission
                    .exported_operations
                    .iter()
                    .filter_map(|operation| submission.wasm_export_names.get(operation).cloned())
                    .collect(),
                output_dir,
            }))
        }
        other => Err(miette!(
            "目标 `{other:?}` 在当前构建中没有已启用的正式 lowering；拒绝切换目标或补造产物"
        )),
    }
}


pub(crate) fn write_wasm_wat_sidecar(output_dir: &Path, artifact_name: &str, input: &DriverBackendInput) -> Result<Option<PathBuf>> {
    let DriverBackendInput::Wasm(input) = input
    else {
        return Ok(None);
    };

    std::fs::create_dir_all(output_dir)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to create WAT sidecar directory: {}", output_dir.display()))?;

    let sidecar_path = output_dir.join(format!("{artifact_name}.wat"));
    let wat_text = render_wasm_module_as_wat(&input.module);
    std::fs::write(&sidecar_path, wat_text)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to write WAT sidecar: {}", sidecar_path.display()))?;
    Ok(Some(sidecar_path))
}

fn render_wasm_module_as_wat(module: &WasmBinaryModule) -> String {
    let mut output = String::from("(module");
    for section in &module.sections {
        output.push('\n');
        output.push_str("  ;; ");
        if section.id == 0 {
            let name = section.name.as_deref().unwrap_or("");
            output.push_str("custom section ");
            output.push_str(&format_quoted_text(name));
            if section.bytes.is_empty() {
                output.push_str(": empty");
            }
            else {
                output.push_str(": ");
                output.push_str(&format_wasm_payload(&section.bytes));
            }
        }
        else {
            output.push_str(&format!("section id {}: {} bytes", section.id, section.bytes.len()));
        }
    }
    output.push('\n');
    output.push(')');
    output
}

fn format_wasm_payload(bytes: &[u8]) -> String {
    let hex = bytes.iter().map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(" ");
    match std::str::from_utf8(bytes) {
        Ok(text) if !text.is_empty() && text.chars().all(|ch| !ch.is_control()) => {
            format!("utf8 {} | hex [{}]", format_quoted_text(text), hex)
        }
        _ => format!("hex [{}]", hex),
    }
}

fn format_quoted_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            _ => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

pub(crate) fn sanitize_symbol(name: &str) -> String {
    // Non-alphanumeric chars must stay distinguishable: `infix ==` and `infix !=`
    // must not both collapse to `infix___` (duplicate MethodDef → BadImage / PEVerify).
    // Keep `_` as-is so already-sanitized / snake_case symbols are stable.
    let mut sanitized = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            sanitized.push(ch);
        }
        else if ch == '.' {
            sanitized.push('_');
        }
        else {
            sanitized.push_str(&format!("_x{:02x}", u32::from(ch)));
        }
    }
    if sanitized.is_empty() { "module".to_string() } else { sanitized }
}

/// Sanitize a resolved operation for MSIL MethodDef / Call names.
///
/// Joins [`QualifiedName`] parts with `.` (same shape as MIR instance symbols /
/// [`NamePath`] Display) before flattening, so `Option::is_none` and
/// `Option.is_none` both become `Option_is_none` — never `Option__is_none`.
pub(crate) fn sanitize_operation_symbol(operation: &nyar::QualifiedName) -> String {
    let dotted = operation.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>().join(".");
    sanitize_symbol(&dotted)
}

/// JVM メソッド名用サニタイズ：`QualifiedName` パーツを `__` でジョインする。
///
/// `demo::main` → `["demo", "main"]` → join `__` → `"demo__main"`
/// `sanitize_operation_symbol`（`.` join → `demo_main`）とは区別する。
pub(crate) fn sanitize_jvm_method_symbol(operation: &nyar::QualifiedName) -> String {
    let underscored = operation.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>().join("__");
    sanitize_symbol(&underscored)
}
