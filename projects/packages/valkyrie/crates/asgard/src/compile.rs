//! Compile V sources to host bytecode for a target family.

use std::{fs, path::Path};

use legion_workspace::{
    compile_source_snapshot,
    planner::{BuildRequest, WorkspaceResolver},
};
use miette::{IntoDiagnostic, Result, WrapErr};
use nyar::ArtifactKind;
use nyar_types::compiler_input::{CompilerSourceFile, CompilerSourceGroup, CompilerSourceInput, GeneratedSourceGroup};
use rusty_valkyrie::{CanonicalTarget, CompilerBuildContext, compile_source_groups_to_artifacts};

use crate::{
    host_backend::HostBackend,
    wasm::{WasmCompileReport, copy_wasm_artifacts_to_dist},
};

/// Host artifact kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostArtifactKind {
    /// WebAssembly module.
    Wasm,
    /// JVM `.class`.
    JvmClass,
    /// Native executable.
    NativeExecutable,
}
/// Host compile report.
#[derive(Debug, Clone)]
pub struct HostCompileReport {
    /// Primary artifact bytes.
    pub artifact_bytes: Vec<u8>,
    /// Artifact kind.
    pub kind: HostArtifactKind,
    /// WASM-specific report (browser).
    pub wasm: Option<WasmCompileReport>,
}

/// Resolver 形成的完整源码快照。
#[derive(Debug, Clone)]
pub struct ResolvedCompilerSources {
    /// 应用组身份。
    pub project_name: String,
    /// 按依赖顺序排列的源码组。
    pub groups: Vec<CompilerSourceGroup>,
}

/// 从 manifest 与当前源码闭包形成 Compiler 输入。
pub fn resolve_project_source_groups(project_dir: &Path, target: &CanonicalTarget) -> Result<ResolvedCompilerSources> {
    let resolver = WorkspaceResolver::discover_for_project(project_dir).map_err(|error| miette::miette!("解析项目工作区失败: {error}"))?;
    let plan = resolver
        .build_plan(&BuildRequest { project_dir: project_dir.to_path_buf(), target: target.clone(), output_dir: None })
        .map_err(|error| miette::miette!("解析项目构建计划失败: {error}"))?;
    let project_name = plan.project.name.clone();
    let groups =
        compile_source_snapshot(&plan.project.semantic_source_groups).map_err(|error| miette::miette!("读取项目源码快照失败: {error}"))?;
    Ok(ResolvedCompilerSources { project_name, groups })
}

/// 终端 TUI 运行时的唯一来源：项目依赖闭包中 `std` 包的 `terminal/tui.v`。
///
/// asgard 不携带标准库源码。终端 `app.v` 的运行时前导必须从已解析的源码组读取；
/// 缺失时返回结构化失败，由 package graph 补齐依赖，而不是在 CLI 内嵌一份副本。
pub fn terminal_runtime_source(groups: &[CompilerSourceGroup]) -> Result<&str> {
    groups
        .iter()
        .flat_map(|group| group.sources.iter())
        .find(|file| file.path.replace('\\', "/").ends_with("terminal/tui.v"))
        .map(|file| file.source.as_str())
        .ok_or_else(|| miette::miette!("项目依赖闭包缺少 std.terminal 运行时源码 terminal/tui.v"))
}

/// 独立生成源码：唯一生成器身份与生成文本。
///
/// 生成文本由 Asgard 从 AWSL 降低产生，作为 Compiler 的独立生成输入参与闭包；
/// 它不会写回应用源码组，也不会在 Compiler 内部被再次拼接或重解析。
#[derive(Debug, Clone, Copy)]
pub struct GeneratedSource<'a> {
    /// 生成器稳定身份；进入 Compiler 证据与 freshness 比对。
    pub producer: &'a str,
    /// 生成源码文本。
    pub source: &'a str,
}

/// 把应用源码组与生成源码组合成 Compiler 的完整输入。
///
/// 生成组显式入口，并只依赖本工程的入口组：生成代码可以看到应用源码的声明，
/// 但不会获得依赖包的额外可见性。
fn compiler_input_for(source_groups: &[CompilerSourceGroup], generated: GeneratedSource<'_>) -> Result<CompilerSourceInput> {
    let entry_keys = source_groups.iter().filter(|group| group.is_entry).map(|group| group.dependency_key.clone()).collect::<Vec<_>>();
    if entry_keys.is_empty() {
        return Err(miette::miette!("源码快照缺少应用入口组，无法挂接生成源码"));
    }
    let group = CompilerSourceGroup {
        dependency_key: "asgard.generated".into(),
        name: "asgard.generated".into(),
        sources: vec![CompilerSourceFile { path: "asgard/generated.v".into(), source: generated.source.into() }],
        direct_dependencies: entry_keys,
        is_entry: true,
    };
    let generated = GeneratedSourceGroup { producer: generated.producer.into(), group };
    Ok(CompilerSourceInput { groups: source_groups.to_vec(), generated: vec![generated] })
}

/// 将已解析的源码组与独立生成源码编译为宿主制品。
pub fn compile_v_bundle(
    source_groups: &[CompilerSourceGroup],
    generated: GeneratedSource<'_>,
    output_dir: &Path,
    module_name: &str,
    target: &CanonicalTarget,
    backend: HostBackend,
) -> Result<HostCompileReport> {
    let compiler = rusty_valkyrie::ValkyrieCompiler::default();
    let target_profile = target.to_profile(None);
    fs::create_dir_all(output_dir).into_diagnostic().wrap_err("failed to create output directory")?;
    let build_context = CompilerBuildContext::new(
        target.arch.as_str(),
        target.clone(),
        nyar::ClrSuspendStrategy::default(),
        nyar_emitter::nyar_backend_wasi::WasmPackageKind::Binary,
    );
    let input = compiler_input_for(source_groups, generated)?;
    let artifact = compile_source_groups_to_artifacts(
        &compiler,
        &input,
        &build_context,
        output_dir,
        module_name,
        true,
        target_profile.artifact_policy.generate_runtime_config,
    )?;
    let report = &artifact.driver;

    match backend {
        HostBackend::BrowserDom => {
            let artifact_name = primary_artifact_name(report, ArtifactKind::Executable)?;
            let safe_name = artifact_name.replace('.', "-");
            let wasm_report = WasmCompileReport { wasm_filename: format!("{safe_name}.wasm"), glue_filename: format!("{safe_name}.mjs") };
            copy_wasm_artifacts_to_dist(output_dir, &wasm_report)?;
            let wasm_path = output_dir.join(&wasm_report.wasm_filename);
            let bytes = fs::read(&wasm_path).into_diagnostic().wrap_err("failed to read wasm")?;
            Ok(HostCompileReport { artifact_bytes: bytes, kind: HostArtifactKind::Wasm, wasm: Some(wasm_report) })
        }
        HostBackend::AndroidCompose => {
            let artifact_name = primary_artifact_name(&report, ArtifactKind::Executable)?;
            let native_path = output_dir.join(format!("{artifact_name}.so"));
            let bytes = fs::read(&native_path).into_diagnostic().wrap_err("Android .so artifact not found")?;
            validate_elf_shared_object(&bytes)?;
            Ok(HostCompileReport { artifact_bytes: bytes, kind: HostArtifactKind::NativeExecutable, wasm: None })
        }
        HostBackend::IosSwiftUi => {
            let artifact_name = primary_artifact_name(&report, ArtifactKind::Executable)?;
            let native_path = output_dir.join(artifact_name);
            let bytes = fs::read(&native_path).into_diagnostic()?;
            Ok(HostCompileReport { artifact_bytes: bytes, kind: HostArtifactKind::NativeExecutable, wasm: None })
        }
        HostBackend::WindowsNative | HostBackend::LinuxNative | HostBackend::MacOsNative => {
            let artifact_name = primary_artifact_name(&report, ArtifactKind::Executable)?;
            let native_path = if matches!(backend, HostBackend::WindowsNative) {
                output_dir.join(format!("{artifact_name}.exe"))
            }
            else {
                output_dir.join(artifact_name)
            };
            let bytes = fs::read(&native_path).into_diagnostic()?;
            Ok(HostCompileReport { artifact_bytes: bytes, kind: HostArtifactKind::NativeExecutable, wasm: None })
        }
        HostBackend::WechatMiniProgram => {
            let artifact_name = primary_artifact_name(&report, ArtifactKind::Executable)?;
            let safe_name = artifact_name.replace('.', "-");
            let wasm_report = WasmCompileReport { wasm_filename: format!("{safe_name}.wasm"), glue_filename: format!("{safe_name}.mjs") };
            copy_wasm_artifacts_to_dist(output_dir, &wasm_report)?;
            let wasm_path = output_dir.join(&wasm_report.wasm_filename);
            let bytes = fs::read(&wasm_path).into_diagnostic().wrap_err("failed to read miniprogram wasm")?;
            Ok(HostCompileReport { artifact_bytes: bytes, kind: HostArtifactKind::Wasm, wasm: Some(wasm_report) })
        }
        HostBackend::Terminal => {
            let artifact_name = primary_artifact_name(&report, ArtifactKind::Executable)?;
            let native_path = output_dir.join(artifact_name);
            let bytes = fs::read(&native_path).into_diagnostic()?;
            Ok(HostCompileReport { artifact_bytes: bytes, kind: HostArtifactKind::NativeExecutable, wasm: None })
        }
    }
}

fn primary_artifact_name(report: &nyar_emitter::DriverCompileReport, kind: ArtifactKind) -> Result<String> {
    report
        .artifacts
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == kind)
        .map(|artifact| artifact.name.clone())
        .ok_or_else(|| miette::miette!("编译报告缺少主产物 `{kind:?}`"))
}

/// 校验 Android 宿主产物为 AArch64 共享库 ELF 且含必需 JNI 符号。
pub fn validate_elf_shared_object(bytes: &[u8]) -> Result<()> {
    if bytes.len() < 20 || &bytes[0..4] != b"\x7fELF" {
        return Err(miette::miette!("Android host artifact is not ELF (need .so)"));
    }
    let e_type = u16::from_le_bytes([bytes[16], bytes[17]]);
    if e_type != 3 {
        return Err(miette::miette!("Android host must be ET_DYN shared library (.so), e_type={e_type}"));
    }
    let e_machine = u16::from_le_bytes([bytes[18], bytes[19]]);
    if e_machine != 183 {
        return Err(miette::miette!("Android host must be AArch64 (EM_AARCH64=183), e_machine={e_machine}"));
    }
    let haystack = String::from_utf8_lossy(bytes);
    for sym in ["JNI_OnLoad", "asgard_invoke_export", "asgard_patch_native"] {
        if !haystack.contains(sym) {
            return Err(miette::miette!("Android .so missing export `{sym}` (JNI must be linked into ASGARDNT)"));
        }
    }
    Ok(())
}
