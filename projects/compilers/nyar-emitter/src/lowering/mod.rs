use std::path::{Path, PathBuf};

use crate::nyar_backend_wasi::{WasmBinaryBackendInput, WasmBinaryModule};
#[cfg(feature = "legacy-lanes")]
use crate::{
    nyar_backend_clr::{ClrBinaryBackendInput, MsilTextWriter},
    nyar_backend_jvm::{JvmBinaryBackendInput, JvmClassFile, JvmInstruction},
    nyar_backend_native::NativeBinaryBackendInput,
};
use miette::{IntoDiagnostic, Result, WrapErr, miette};
#[cfg(feature = "legacy-lanes")]
use nyar::QualifiedName;
use nyar::{HostProjectionBoundary, TargetBackendFamily};
#[cfg(feature = "legacy-lanes")]
use nyar_types::{AggregateLayoutPlan, FlagsLayout, SumTypeLayout};
#[cfg(feature = "legacy-lanes")]
use std_data::binary::{elf::NativeElfImageBuilder, nyar_ir::NyarModuleData, pe::NativeImageBuilder};

use crate::{DriverBackendInput, FragmentSubmission};
#[cfg(feature = "legacy-lanes")]
use crate::{NyarVmBackendInput, executable_provider::ExecutableFunction};

pub(crate) mod backends;
pub(crate) mod features;
mod shared;
mod tooling;

#[cfg(feature = "legacy-lanes")]
pub(crate) use self::backends::clr_types;
pub(crate) use self::features::pattern_matching_contract;

#[cfg(not(feature = "legacy-lanes"))]
use self::backends::wasm;
#[cfg(feature = "legacy-lanes")]
use self::{
    backends::{clr, clr_suspend, clr_witness, jvm, native, nyar_vm, wasm},
    features::singleton::{augment_msil_with_singletons, build_jvm_singleton_classes, build_jvm_struct_classes},
};

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_lower_fragment_to_clr_msil(submission: &FragmentSubmission) -> Result<crate::nyar_backend_clr::MsilModule> {
    clr::lower_fragment_to_msil(submission)
}

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_lower_mir_to_clr_method(
    submission: &FragmentSubmission,
    operation: &QualifiedName,
    mir_function: &ExecutableFunction,
) -> Result<crate::nyar_backend_clr::MsilMethodBody> {
    backends::clr_mir::lower_mir_function_to_msil(submission, operation, mir_function)
}

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_lower_mir_to_jvm_method(
    submission: &FragmentSubmission,
    operation: &QualifiedName,
    mir_function: &ExecutableFunction,
) -> crate::nyar_backend_jvm::JvmMethodSignature {
    backends::jvm_mir::lower_mir_function_to_jvm(submission, operation, mir_function)
}

pub(crate) fn testing_lower_fragment_to_wasm_mir_module(submission: &FragmentSubmission, export_name: &str) -> WasmBinaryModule {
    backends::wasm::mir::lower_fragment_mir_to_wasm_module(submission, export_name).0
}

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_build_clr_type_defs(plan: &AggregateLayoutPlan) -> Vec<crate::nyar_backend_clr::MsilTypeDef> {
    backends::clr_types::build_clr_type_defs(plan)
}

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_build_clr_nominal_type_defs(
    sum_types: &[SumTypeLayout],
    flags_types: &[FlagsLayout],
) -> Vec<crate::nyar_backend_clr::MsilTypeDef> {
    backends::clr_nominal::build_clr_nominal_type_defs(sum_types, flags_types)
}

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_lower_fragment_to_nyar_module(submission: &FragmentSubmission) -> NyarModuleData {
    nyar_vm::lower_fragment_to_nyar_module(submission)
}

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_build_jvm_singleton_classes(submission: &FragmentSubmission) -> Vec<JvmClassFile> {
    build_jvm_singleton_classes(submission)
}

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_lower_fragment_to_jvm_class(submission: &FragmentSubmission) -> Result<JvmClassFile> {
    jvm::lower_fragment_to_jvm_class(submission)
}

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_append_jvm_witness_methods(
    class_file: &mut JvmClassFile,
    submission: &FragmentSubmission,
) -> Option<Vec<JvmInstruction>> {
    backends::jvm_witness::append_witness_methods(class_file, submission)
}

pub(crate) fn testing_decode_wasm_uleb128(bytes: &[u8], pos: &mut usize) -> u32 {
    wasm::decode_uleb128(bytes, pos)
}

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_augment_msil_with_singletons(
    submission: &FragmentSubmission,
    module: &mut crate::nyar_backend_clr::MsilModule,
) -> Result<()> {
    augment_msil_with_singletons(submission, module)
}

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_augment_msil_with_witness(
    submission: &FragmentSubmission,
    module: &mut crate::nyar_backend_clr::MsilModule,
) -> Result<()> {
    clr_witness::augment_msil_with_witness(submission, module)
}

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_augment_msil_with_suspend(submission: &FragmentSubmission, module: &mut crate::nyar_backend_clr::MsilModule) {
    clr_suspend::augment_msil_with_suspend(submission, module);
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

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_lower_fragment_to_native_executable(submission: &FragmentSubmission, host_flavor: &str) -> Result<(Vec<u8>, String)> {
    native::lower_fragment_to_native_executable(submission, host_flavor)
}

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_lower_mir_functions_to_native_msvc(
    submission: &FragmentSubmission,
    function: &mut std_data::binary::x86_64::MsvcFunctionBuilder,
) {
    native::lower_mir_functions_to_native_msvc(submission, function)
}

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_lower_mir_functions_to_native_sysv(
    submission: &FragmentSubmission,
    function: &mut std_data::binary::x86_64::SysvFunctionBuilder,
) {
    native::lower_mir_functions_to_native_sysv(submission, function)
}

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_lower_suspend_witness_calls_windows(
    submission: &FragmentSubmission,
    function: &mut std_data::binary::x86_64::MsvcFunctionBuilder,
    builder: &mut NativeImageBuilder,
) {
    native::lower_suspend_witness_calls_windows(submission, function, builder)
}

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_lower_suspend_witness_calls_linux(
    submission: &FragmentSubmission,
    function: &mut std_data::binary::x86_64::SysvFunctionBuilder,
    builder: &mut NativeElfImageBuilder,
) {
    native::lower_suspend_witness_calls_linux(submission, function, builder)
}

#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_emit_native_witness_tables(
    pe: Option<&mut NativeImageBuilder>,
    elf: Option<&mut NativeElfImageBuilder>,
    submission: &FragmentSubmission,
) -> Result<()> {
    backends::witness::emit_witness_tables(pe, elf, submission)
}

pub(crate) fn testing_mir_lowering_context(submission: &FragmentSubmission) -> shared::executable::ExecutableLoweringContext<'_> {
    shared::executable::ExecutableLoweringContext::new(submission)
}

pub(crate) fn testing_field_slot_index(layouts: &FragmentSubmission, layout_id: Option<u32>, type_name: &str, field: &str) -> u16 {
    shared::executable::ExecutableLoweringContext::new(layouts).field_slot_index(layout_id, type_name, field)
}

pub(crate) const TESTING_WASM_GC_ANYREF: u8 = wasm::WASM_GC_ANYREF;
#[cfg(feature = "legacy-lanes")]
pub(crate) const TESTING_NATIVE_VALUE_AREA_BASE: i32 = native::NATIVE_VALUE_AREA_BASE;
#[cfg(feature = "legacy-lanes")]
pub(crate) const TESTING_SUSPEND_SPILL_RSP_OFFSET: i32 = native::SUSPEND_SPILL_RSP_OFFSET;
#[cfg(feature = "legacy-lanes")]
pub(crate) fn testing_native_value_area_size(submission: &FragmentSubmission) -> u32 {
    native::native_value_area_size(submission)
}

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
) -> Result<DriverBackendInput> {
    features::semantic_mir_contract::validate_submission(submission)
        .map_err(|error| miette!("semantic MIR contract failed [{}] {} at {}: {}", error.code, error.function, error.location, error.detail))?;
    match backend_family {
        #[cfg(feature = "legacy-lanes")]
        TargetBackendFamily::Clr => {
            let mut module = clr::lower_fragment_to_msil(submission)?;
            augment_msil_with_singletons(submission, &mut module)?;
            clr_witness::augment_msil_with_witness(submission, &mut module)?;
            clr_suspend::augment_msil_with_suspend(submission, &mut module);
            clr::reject_unresolved_local_calls(&module)?;
            Ok(DriverBackendInput::Clr(ClrBinaryBackendInput { module, output_dir, image_kind: None }))
        }
        #[cfg(feature = "legacy-lanes")]
        TargetBackendFamily::Jvm => {
            let mut companion_classes = build_jvm_singleton_classes(submission);
            companion_classes.extend(build_jvm_struct_classes(submission));
            Ok(DriverBackendInput::Jvm(JvmBinaryBackendInput {
                class_file: jvm::lower_fragment_to_jvm_class(submission)?,
                output_dir,
                emit_class_file: true,
                control_flow: submission.control_flow.clone(),
                companion_classes,
            }))
        }
        TargetBackendFamily::Wasm => {
            let wasi_preview = crate::nyar_backend_wasi::WasiPreview::from_host_flavor(host_flavor);
            let (module, imports) = wasm::lower_fragment_to_wasm_module_for(submission, host_boundary, wasi_preview)?;
            // WASI component model: `_start` is always nullary (argv is read via
            // `wasi:cli/environment.get-arguments`), so any WASI partition with an
            // executable entry should be packaged as a command component.
            let has_executable_entry = submission.executable.as_ref().is_some_and(|exec| !exec.operations().is_empty());
            let package_as_wasi_command = host_boundary == HostProjectionBoundary::WasiComponent && has_executable_entry;
            Ok(DriverBackendInput::Wasm(WasmBinaryBackendInput {
                module,
                output_dir,
                host_boundary,
                imports,
                control_flow: submission.control_flow.clone(),
                package_as_wasi_command,
                wasi_preview,
            }))
        }
        #[cfg(feature = "legacy-lanes")]
        TargetBackendFamily::Native => {
            let (executable, entry_symbol) = native::lower_fragment_to_native_executable(submission, host_flavor)?;
            Ok(DriverBackendInput::Native(NativeBinaryBackendInput { executable, output_dir, entry_symbol }))
        }
        #[cfg(feature = "legacy-lanes")]
        TargetBackendFamily::NyarVm => {
            let nyar_module = if submission.suspend_runtime.is_some() && submission.exported_operations.is_empty() {
                None
            }
            else {
                Some(nyar_vm::lower_fragment_to_nyar_module(submission))
            };
            Ok(DriverBackendInput::NyarVm(NyarVmBackendInput {
                suspend_runtime: submission.suspend_runtime.clone(),
                control_flow: submission.control_flow.clone(),
                nyar_module,
                output_dir,
            }))
        }
        other => Err(miette!(
            "`{other:?}` lane is frozen for 0.0.x Node/Wasm delivery; enable Cargo feature `legacy-lanes` to compile CLR/JVM/native/nyar_vm lowering"
        )),
    }
}

#[cfg(feature = "legacy-lanes")]
pub(crate) fn write_clr_msil_sidecar(output_dir: &Path, artifact_name: &str, input: &DriverBackendInput) -> Result<Option<PathBuf>> {
    let DriverBackendInput::Clr(input) = input
    else {
        return Ok(None);
    };

    std::fs::create_dir_all(output_dir)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to create MSIL sidecar directory: {}", output_dir.display()))?;

    let sidecar_path = output_dir.join(format!("{artifact_name}.msil"));
    let msil_text = MsilTextWriter::new().write_module(&input.module);
    std::fs::write(&sidecar_path, &msil_text)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to write MSIL sidecar: {}", sidecar_path.display()))?;
    Ok(Some(sidecar_path))
}

#[cfg(not(feature = "legacy-lanes"))]
pub(crate) fn write_clr_msil_sidecar(_output_dir: &Path, _artifact_name: &str, _input: &DriverBackendInput) -> Result<Option<PathBuf>> {
    Ok(None)
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
