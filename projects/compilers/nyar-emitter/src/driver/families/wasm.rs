use crate::nyar_backend_wasi::WasmBinaryBackend;
use miette::Result;
use nyar::{HostProjectionBoundary, PartitionBackendRequirement, TargetBackendFamily, backends::TargetCodeGenBackend};

use super::BundledBackendCompiler;
use crate::{DriverBackendInput, DriverCompileReport, DriverCompileRequest, DriverRunContract, bundled_backend_capability_descriptor};

/// `WASM` 家族后端编译器，统一调度 `WasmJsGlue` 与 `WasiComponent` 两种宿主边界。
pub(super) struct WasmFamilyCompiler;

/// 判断给定的后端需求是否可由 `WASM` 家族满足。
pub(super) fn supports_requirement(requirement: &PartitionBackendRequirement) -> bool {
    bundled_backend_capability_descriptor(TargetBackendFamily::Wasm).is_some_and(|descriptor| descriptor.supports_requirement(requirement))
}

impl BundledBackendCompiler for WasmFamilyCompiler {
    fn compile(&self, request: DriverCompileRequest<'_>) -> Result<DriverCompileReport> {
        let DriverBackendInput::Wasm(input) = request.input
        else {
            return Err(miette::miette!("`WASM` 家族请求必须携带 `WasmBinaryBackendInput`"));
        };
        let expected_boundary = input.host_boundary;
        if request.requirement.host_boundary != expected_boundary {
            return Err(miette::miette!(
                "`WASM` 后端输入宿主与规划需求不一致：input={:?}, requirement={:?}",
                expected_boundary,
                request.requirement.host_boundary
            ));
        }
        let backend = WasmBinaryBackend::new();
        backend.validate(&input)?;
        let package_as_wasi_command = input.package_as_wasi_command;
        let wasi_preview = input.wasi_preview;
        let artifacts = backend.compile(input, request.options)?;
        let run_contracts = match request.requirement.host_boundary {
            HostProjectionBoundary::WasiComponent if !package_as_wasi_command => Vec::new(),
            boundary => vec![wasm_run_contract(request.artifact_name, boundary, wasi_preview)],
        };
        Ok(DriverCompileReport { artifacts, entry_symbol: None, run_contracts })
    }
}

fn wasm_run_contract(
    artifact_name: &str,
    boundary: HostProjectionBoundary,
    wasi_preview: crate::nyar_backend_wasi::WasiPreview,
) -> DriverRunContract {
    let physical_name = wasm_physical_artifact_name(artifact_name);
    match boundary {
        HostProjectionBoundary::WasmJsGlue => DriverRunContract {
            logical_entry: "main".to_string(),
            physical_entry: format!("{}.mjs", physical_name),
            invocation: "node".to_string(),
            validate: format!("node {}.mjs", physical_name),
        },
        HostProjectionBoundary::WasiComponent => {
            let p3_flag = match wasi_preview {
                crate::nyar_backend_wasi::WasiPreview::Preview3 => " -S p3",
                crate::nyar_backend_wasi::WasiPreview::Preview2 => "",
            };
            DriverRunContract {
                logical_entry: "_start".to_string(),
                physical_entry: format!("{}.wasi", physical_name),
                invocation: "wasmtime".to_string(),
                validate: format!("wasmtime run -W gc -W wmemcheck -W max-memory-size=16777216{p3_flag} {}.wasi", physical_name),
            }
        }
        other => unreachable!("unexpected wasm host boundary: {:?}", other),
    }
}

/// 将逻辑产物名规范化为 `WASM` 物理产物名。
///
/// 通用规则：直接使用 `artifact_name` 作为物理产物名，不对任何项目名做特殊分支。
/// 此函数是 `DriverRunContract` 引用物理文件名的唯一来源，必须与
/// `backend::wasi::canonical_wasm_artifact_stem` 产出相同的结果。
fn wasm_physical_artifact_name(artifact_name: &str) -> String {
    artifact_name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 `wasm_physical_artifact_name` 对任意项目名原样返回，无特殊分支。
    #[test]
    fn physical_name_preserves_artifact_names() {
        assert_eq!(wasm_physical_artifact_name("demo"), "demo");
        assert_eq!(wasm_physical_artifact_name("legion"), "legion");
        assert_eq!(wasm_physical_artifact_name("legion.tools"), "legion.tools");
        assert_eq!(wasm_physical_artifact_name("main_legion"), "main_legion");
        assert_eq!(wasm_physical_artifact_name("wasm_interop_stdout"), "wasm_interop_stdout");
        assert_eq!(wasm_physical_artifact_name("alpha_entry__main_alpha_entry"), "alpha_entry__main_alpha_entry");
    }
}
