//! `WebAssembly` 二进制后端容器入口，覆盖 `WasmJsGlue` 与 `WasiComponent` 两种宿主边界。
//!
//! 这里按 `wasm / wat / wit` 三个输出格式收口，
//! 相关格式模型与编解码统一由 `std-data` 提供。

#![warn(missing_docs)]

mod component;
mod witness_dispatch;

use std::{collections::BTreeSet, path::PathBuf};

use miette::{IntoDiagnostic, Result, WrapErr};
use nyar::{
    ControlFlowPayload, HostProjectionBoundary,
    abstractions::{ArtifactFormat, BackendInputKind, BinaryTarget},
    backends::{BackendDescriptor, CompilationOptions, TargetCodeGenBackend},
    packaging::{ArtifactDescriptor, ArtifactSet, TargetLane},
};

use crate::backend::binding_builders::{BindingGenerationContext, generate_host_binding_artifacts};
use std_data::binary::wasm::{parse_export_section, parse_import_section};

pub use component::WasiPreview;
pub(crate) use component::{
    WASI_PREVIEW2_VERSION, WASI_PREVIEW3_VERSION, WitBindingBuilder, package_core_wasm_as_component, wasi_adapt_import_for_preview,
    wasi_cli_run_export_name, wasi_cli_run_export_name_for, wasi_versioned_import_module, wasi_versioned_import_module_for,
    write_component_wit_package, write_component_wit_package_for,
};
pub use std_data::{
    binary::wasm::{WasmBinaryError, WasmBinaryModule, WasmCustomSection, WasmSection},
    text::{
        wat::{WatDocument, WatError},
        wit::{WitError, WitInterface, WitPackage},
    },
};
pub use witness_dispatch::{
    WasmTraitFatPointer, WitnessMethodSlot, WitnessTableLayout, materialize_witness_bytes, plan_witness_table_layout, resolve_witness_call,
};

/// Returns the physical WASM/WASI artifact stem from the configured artifact
/// name. This boundary deliberately preserves the name verbatim: project or
/// library names are not semantic evidence and must never select a special
/// bootstrap path.
fn canonical_wasm_artifact_stem(artifact_name: &str) -> String {
    artifact_name.to_string()
}

/// `WebAssembly` 二进制后端输入（覆盖 `WasmJsGlue` 与 `WasiComponent` 宿主边界）。
#[derive(Debug, Clone)]
pub struct WasmBinaryBackendInput {
    /// `WASM` 模块。
    pub module: WasmBinaryModule,
    /// 输出目录。
    pub output_dir: PathBuf,
    /// 宿主投影边界。
    pub host_boundary: HostProjectionBoundary,
    /// 导入声明列表（`(module, field)` 对），由宿主绑定生成器消费以生成对应的 `import` 实现。
    pub imports: Vec<(String, String)>,
    /// 可选的挂起产物，用于驱动层线路；二进制后端忽略此字段。
    pub control_flow: Option<ControlFlowPayload>,
    /// Whether this fragment's entry matches WASI command `run: func()` (nullary).
    ///
    /// Param-taking entries stay as core wasm only; command components are reserved
    /// for nullary entries that read argv via `wasi:cli`.
    pub package_as_wasi_command: bool,
    /// WASI package-train selection (`wasip2` / `wasip3`). Ignored for JS glue.
    pub wasi_preview: WasiPreview,
}

/// `WebAssembly` 二进制后端（覆盖 `WasmJsGlue` 与 `WasiComponent` 宿主边界）。
pub struct WasmBinaryBackend {
    descriptor: BackendDescriptor,
}

impl WasmBinaryBackend {
    /// 创建一个新的 `WebAssembly` 二进制后端。
    pub fn new() -> Self {
        Self {
            descriptor: BackendDescriptor {
                name: "wasm-binary".to_string(),
                input_kind: BackendInputKind::WasmModule,
                supported_targets: vec![BinaryTarget::new(nyar::TargetFamily::Wasm, nyar::BinaryArch::Any, nyar::BinaryFlavor::Native)],
            },
        }
    }
}

impl Default for WasmBinaryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl TargetCodeGenBackend for WasmBinaryBackend {
    type Input = WasmBinaryBackendInput;

    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn validate(&self, input: &Self::Input) -> Result<()> {
        match input.host_boundary {
            HostProjectionBoundary::WasmJsGlue | HostProjectionBoundary::WasiComponent => {}
            other => return Err(miette::miette!("`WASM` 后端只支持 `WasmJsGlue/WasiComponent`，实际得到 {:?}", other)),
        };

        // This is intentionally a physical gate, separate from Semantic MIR
        // validation: malformed section bytes or a stale glue-import list must
        // be rejected before any artifact is written.
        let bytes = input.module.to_bytes().map_err(|error| miette::miette!("WASM 预发射编码失败：{error}"))?;
        let decoded = WasmBinaryModule::from_bytes(&bytes).map_err(|error| miette::miette!("WASM 预发射结构验证失败：{error}"))?;
        let actual_imports: BTreeSet<_> = parse_import_section(&decoded).into_iter().map(|item| (item.module, item.field)).collect();
        for expected in &input.imports {
            let physical = wasi_core_import_name(expected, input.wasi_preview);
            if !actual_imports.contains(&physical) {
                return Err(miette::miette!(
                    "WASM glue import metadata declares `{}.{}`, but core module does not import it",
                    expected.0,
                    expected.1
                ));
            }
        }
        let exports: BTreeSet<_> = parse_export_section(&decoded).into_iter().map(|item| item.name).collect();
        let required_entry = match input.host_boundary {
            HostProjectionBoundary::WasmJsGlue => "main",
            HostProjectionBoundary::WasiComponent if input.package_as_wasi_command => "_start",
            HostProjectionBoundary::WasiComponent => return Ok(()),
            _ => unreachable!(),
        };
        if !exports.contains(required_entry) {
            return Err(miette::miette!("WASM pre-emission verifier: required `{required_entry}` function export is absent"));
        }
        Ok(())
    }

    fn compile(&self, input: Self::Input, options: &CompilationOptions) -> Result<ArtifactSet> {
        std::fs::create_dir_all(&input.output_dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("创建输出目录失败：{}", input.output_dir.display()))?;

        let artifact_stem = canonical_wasm_artifact_stem(&options.artifact_name);
        let wasm_bytes = input.module.to_bytes().map_err(|error| miette::miette!("WASM 写入失败：{error}"))?;
        match input.host_boundary {
            HostProjectionBoundary::WasmJsGlue => {
                let wasm_path = input.output_dir.join(format!("{artifact_stem}.wasm"));
                std::fs::write(&wasm_path, wasm_bytes)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("写入 WASM 文件失败：{}", wasm_path.display()))?;
            }
            HostProjectionBoundary::WasiComponent => {
                let core_wasm_path = input.output_dir.join(format!("{artifact_stem}.core.wasm"));
                std::fs::write(&core_wasm_path, wasm_bytes)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("写入 core WASM 文件失败：{}", core_wasm_path.display()))?;
                // WASI command world requires nullary `run`. Param-taking entries
                // emit core only; a separate nullary entry that reads wasi:cli argv
                // becomes the `.wasi` command component.
                if input.package_as_wasi_command {
                    let component_path = input.output_dir.join(format!("{artifact_stem}.wasi"));
                    let wit_package_path =
                        write_component_wit_package_for(&input.output_dir, &artifact_stem, &input.imports, input.wasi_preview)?;
                    package_core_wasm_as_component(&core_wasm_path, &wit_package_path, &component_path)?;
                }
            }
            other => return Err(miette::miette!("`WASM` 后端不支持的 host boundary：{:?}", other)),
        }

        let mut artifacts = ArtifactSet::default();
        artifacts.push(ArtifactDescriptor {
            name: artifact_stem.clone(),
            kind: nyar::ArtifactKind::Executable,
            format: ArtifactFormat::RawBinary,
            target: options.target.clone(),
            lane: TargetLane::Wasm,
        });

        let binding_artifacts = generate_host_binding_artifacts(
            input.host_boundary,
            BindingGenerationContext {
                artifact_name: &artifact_stem,
                output_dir: &input.output_dir,
                target: &options.target,
                imports: &input.imports,
                wasi_preview: input.wasi_preview,
            },
        )?;
        for artifact in binding_artifacts {
            artifacts.push(artifact);
        }

        Ok(artifacts)
    }
}

fn wasi_core_import_name(import: &(String, String), preview: WasiPreview) -> (String, String) {
    if preview == WasiPreview::Preview2 {
        let bare = import.0.split('@').next().unwrap_or(import.0.as_str());
        let module = match bare {
            "wasi:clocks/monotonic-clock" => "cm32p2|wasi:clocks/monotonic-clock@0.2",
            "wasi:io/streams" => "cm32p2|wasi:io/streams@0.2",
            _ => return import.clone(),
        };
        return (module.to_string(), import.1.clone());
    }
    import.clone()
}

#[cfg(test)]
mod tests {
    use super::canonical_wasm_artifact_stem;

    #[test]
    fn artifact_stem_preserves_configured_name_without_project_special_cases() {
        for name in ["demo", "legion", "legion.tools", "legion_tools", "another.library"] {
            assert_eq!(canonical_wasm_artifact_stem(name), name);
        }
    }
}
