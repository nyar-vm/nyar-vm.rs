// `legion build` 命令。
//
// 这里只负责装配与落盘；目标解释、后端注册和 lowering 均交给 `emitter`。

use legion_workspace::{
    labeled_report_with_context,
    manifest::{ProjectArtifactKind, ProjectManifest},
    planner::{BuildPlan, BuildRequest, WorkspaceResolver},
    source_snapshot::compile_source_snapshot,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{Arc, mpsc},
    thread,
};

use clap::Args;
use miette::{IntoDiagnostic, Report, Result, WrapErr, miette};
use nyar::ClrSuspendStrategy;
use nyar_emitter::DriverRunContract;
use nyar_types::compiler_input::CompilerSourceInput;
use rusty_valkyrie::{
    ArtifactSet, CanonicalSpecification, CanonicalTarget, CompilerBuildContext, CompilerHostProviderBinding, compile_source_groups_to_artifacts,
};
use serde::Serialize;

use crate::{
    cache::{CompilationCache, cache_root_for, collect_build_bundle, compute_artifact_hash, store_cached_build},
    cmds::{
        project_input::resolve_project_path,
        run::{ExecutionManifest, RunContract},
    },
    script, unity_export, write_von_indented,
};

/// `legion build` 的命令参数。
#[derive(Debug, Clone, Args)]
pub struct BuildArgs {
    /// 项目目录，默认当前目录。
    #[arg(value_name = "project-dir", default_value = ".")]
    pub project_dir: PathBuf,
    /// 目标平台。
    #[arg(long, default_value = "clr")]
    pub target: CanonicalTarget,
    /// 输出目录。
    #[arg(short = 'o', long = "output")]
    pub output_dir: Option<PathBuf>,
    /// 强制按 workspace 成员解析；若项目未注册则直接报错，不回退到 package 模式。
    #[arg(long, default_value_t = false)]
    pub workspace: bool,
    /// 输出调试用构建副产物。
    #[arg(long, default_value_t = false)]
    pub debug_artifacts: bool,
}

/// 执行 `legion build`。
pub fn run(args: &BuildArgs) -> Result<ExitCode> {
    if args.workspace {
        return run_workspace_members_in_parallel(args);
    }

    let project_input = resolve_project_path(&args.project_dir)?;
    let workspace = WorkspaceResolver::discover_for_project(&project_input)?;
    if workspace.is_workspace_only_root(&project_input) {
        return run_workspace_members_in_parallel(args);
    }

    let request = BuildRequest { project_dir: project_input, target: args.target.clone(), output_dir: args.output_dir.clone() };
    let plan = workspace.build_plan(&request)?;

    println!("workspace: {}", crate::cmds::path_for_cli_log(&plan.workspace_root));
    println!("mode: workspace");
    println!("project: {}", plan.project.name);
    println!("target: {}", plan.project.build_target.target);
    println!("output: {}", crate::cmds::path_for_cli_log(&plan.output_dir));
    println!("sources: {}", plan.project.source_files.len());
    println!("host contracts: {}", plan.project.host_contracts.len());
    println!("selected host providers: {}", plan.project.selected_host_providers.len());

    if plan.project.source_files.is_empty() {
        return Err(miette!("没有找到任何源码文件"));
    }

    if args.debug_artifacts {
        write_host_selection_spec(&plan.output_dir, &plan.project.selected_host_providers)?;
    }
    else {
        remove_host_selection_spec(&plan.output_dir)?;
    }

    let report = compile_plan(&plan, true)?;
    println!("backend status: compiled");
    let target_profile = plan.project.build_target.target.to_profile(None);
    println!("host kind: {:?}", target_profile.host_kind);
    println!("publish format: {}", target_profile.artifact_policy.default_publish_format);
    if let Some(entry_symbol) = &report.entry_symbol {
        println!("entry: {}", entry_symbol);
    }
    print_artifacts(&plan.output_dir, &report.artifacts);
    let manifest_source = if script::is_script_path(&plan.project.manifest_path) {
        let script_source = fs::read_to_string(&plan.project.manifest_path)
            .into_diagnostic()
            .wrap_err_with(|| format!("读取单脚本失败：{}", plan.project.manifest_path.display()))?;
        script::extract_embedded_manifest(&script_source, &plan.project.manifest_path)
            .map_err(|error| Report::from(error))?
            .ok_or_else(|| miette!("单脚本 `{}` 缺少内嵌 `# ```legion` 块", plan.project.manifest_path.display()))?
    }
    else {
        fs::read_to_string(&plan.project.manifest_path)
            .into_diagnostic()
            .wrap_err_with(|| format!("读取项目清单失败：{}", plan.project.manifest_path.display()))?
    };
    let project_manifest = ProjectManifest::parse(&manifest_source)?;
    if let Some(plugin) = &project_manifest.build_plugin {
        unity_export::export_unity_project(&plan, &report, plugin)?;
        println!("unity export: {}", plugin.kind);
    }

    Ok(ExitCode::SUCCESS)
}

fn run_workspace_members_in_parallel(args: &BuildArgs) -> Result<ExitCode> {
    let workspace = WorkspaceResolver::discover(&args.project_dir)?;
    let requested_target = args.target.clone();
    let members = workspace
        .member_manifest_dirs()
        .into_iter()
        .filter(|project_dir| {
            let request =
                BuildRequest { project_dir: project_dir.clone(), target: requested_target.clone(), output_dir: args.output_dir.clone() };
            match workspace.build_plan(&request) {
                Ok(_) => true,
                Err(legion_workspace::planner::PlannerError::MissingBuildTarget { .. }) => {
                    eprintln!("workspace: skipping {} (target not declared)", project_dir.display());
                    false
                }
                Err(error) => {
                    eprintln!("workspace: retaining {} after planner error: {}", project_dir.display(), error);
                    true
                }
            }
        })
        .collect::<Vec<_>>();
    if members.is_empty() {
        return Err(miette!("workspace 没有可构建成员"));
    }

    let worker_count = thread::available_parallelism().map(|value| value.get()).unwrap_or(4).min(members.len());
    println!("workspace mode: 并行构建 {} 个成员 (workers={worker_count})", members.len());

    let queue = Arc::new(std::sync::Mutex::new(members));
    let (tx, rx) = mpsc::channel::<(PathBuf, std::result::Result<ExitCode, String>)>();
    let mut handles = Vec::new();

    for _ in 0..worker_count {
        let queue = Arc::clone(&queue);
        let tx = tx.clone();
        let target = args.target.clone();
        let output = args.output_dir.clone();
        let debug_artifacts = args.debug_artifacts;
        // Backend lowering can recurse deeply over large workspaces. Keep the
        // debug seed usable without relying on a release build's larger stack.
        handles.push(
            thread::Builder::new()
                .name("legion-build-worker".into())
                .stack_size(64 * 1024 * 1024)
                .spawn(move || {
                    loop {
                        let next = {
                            let mut guard = queue.lock().expect("workspace queue poisoned");
                            guard.pop()
                        };
                        let Some(project_dir) = next
                        else {
                            break;
                        };

                        let member_args = BuildArgs {
                            project_dir: project_dir.clone(),
                            target: target.clone(),
                            output_dir: output.clone(),
                            workspace: false,
                            debug_artifacts,
                        };
                        let result = match run(&member_args) {
                            Ok(code) => Ok(code),
                            Err(error) => Err(error.to_string()),
                        };
                        let _ = tx.send((project_dir, result));
                    }
                })
                .map_err(|error| miette!("failed to spawn workspace build worker: {error}"))?,
        );
    }
    drop(tx);

    let mut failures = Vec::new();
    for (project_dir, result) in rx {
        match result {
            Ok(code) if code == ExitCode::SUCCESS => {}
            Ok(_) => failures.push(format!("{}: exited with failure code", project_dir.display())),
            Err(error) => failures.push(format!("{}: {}", project_dir.display(), error)),
        }
    }
    for handle in handles {
        let _ = handle.join();
    }

    if failures.is_empty() {
        println!("workspace 构建完成：全部成功");
        Ok(ExitCode::SUCCESS)
    }
    else {
        for failure in &failures {
            eprintln!("workspace build failure: {failure}");
        }
        Ok(ExitCode::FAILURE)
    }
}

/// 编译给定 `BuildPlan`，返回驱动编译报告。
pub fn compile_plan(plan: &BuildPlan, verbose: bool) -> Result<nyar_emitter::DriverCompileReport> {
    if plan.project.source_files.is_empty() {
        return Err(miette!("没有找到任何源码文件"));
    }

    let cache = CompilationCache::open(cache_root_for(&plan.workspace_root));
    let canonical_triple = plan.project.build_target.target.as_canonical_str();
    let manifest_files = plan
        .project
        .semantic_source_groups
        .iter()
        .map(|group| group.manifest_dir.join("legion.von"))
        .chain(std::iter::once(plan.project.manifest_path.clone()))
        .collect::<Vec<_>>();
    let ir_hash = compute_artifact_hash(
        &plan.project.source_files,
        &manifest_files,
        &canonical_triple,
        plan.project.build_target.wat,
        plan.project.build_target.runtime_async,
    )
    .map_err(|error| miette!("{error}"))?;

    // WASI 与 Node 同为 wasm32，但宿主不同：Node 走 `env.*`，WASI 走 guest/host_contract。
    // 模板 `<% match arch %>` 因此对 WASI 使用伪架构键 `"wasi"`，避免误选 wasm32/Node 分支。
    let arch = compile_arch(plan);
    let source_groups = compile_source_snapshot(&plan.project.semantic_source_groups)?;
    if verbose {
        println!("frontend: semantic source groups={}", plan.project.semantic_source_groups.len());
    }
    let target_profile = plan.project.build_target.target.to_profile(None);
    let clr_suspend_strategy = ClrSuspendStrategy::from_runtime_async_flag(plan.project.build_target.runtime_async);
    let wasm_package_kind = wasm_package_kind_for_manifest(plan.project.artifact_kind);
    fs::create_dir_all(&plan.output_dir).into_diagnostic().wrap_err_with(|| format!("创建输出目录失败 {}", plan.output_dir.display()))?;
    let host_bindings = plan
        .project
        .selected_host_providers
        .iter()
        .map(|provider| {
            let contract =
                provider.contract_key.clone().ok_or_else(|| miette!("选中的 host provider `{}` 缺少契约声明身份", provider.symbol))?;
            Ok(CompilerHostProviderBinding { contract, provider: provider.key.clone() })
        })
        .collect::<Result<Vec<_>>>()?;
    let build_context = CompilerBuildContext::new(arch, plan.project.build_target.target.clone(), clr_suspend_strategy, wasm_package_kind)
        .with_selected_host_providers(host_bindings);
    let artifact = compile_source_groups_to_artifacts(
        &rusty_valkyrie::ValkyrieCompiler::default(),
        &nyar_types::compiler_input::CompilerSourceInput::application(source_groups),
        &build_context,
        &plan.output_dir,
        &plan.project.name,
        plan.project.build_target.wat,
        target_profile.artifact_policy.generate_runtime_config,
    )?;
    write_compile_evidence(&plan.output_dir, &artifact.evidence)?;
    if verbose {
        println!("compiler: canonical artifact set ready");
    }

    // Write execution manifest before collecting the artifact-set so restore can run.
    if !artifact.driver.run_contracts.is_empty() {
        write_execution_manifest(plan, &artifact.driver.run_contracts, &artifact.evidence)?;
    }
    else {
        ExecutionManifest::remove_from_output_dir(&plan.output_dir)?;
    }

    let bundle = collect_build_bundle(&plan.output_dir, &artifact.driver).map_err(|error| miette!("产物集合不完整：{error}"))?;
    store_cached_build(&cache, &plan.project.name, &canonical_triple, &ir_hash, &bundle)
        .map_err(|error| miette!("产物缓存写入失败：{error}"))?;
    if verbose {
        println!("cache: stored (artifact-set)");
    }

    Ok(artifact.driver)
}

/// 编译期架构键：WASI 与 Node 同为 wasm32，但宿主不同，因此使用伪架构键 `"wasi"`。
pub(crate) fn compile_arch(plan: &BuildPlan) -> &'static str {
    if plan.project.build_target.target.specification == CanonicalSpecification::Wasi {
        "wasi"
    }
    else {
        plan.project.build_target.target.arch.as_str()
    }
}

fn write_compile_evidence(output_dir: &Path, evidence: &rusty_valkyrie::CompilerCompileEvidence) -> Result<()> {
    let output_path = output_dir.join("compile-evidence.txt");
    let content = crate::write_von_indented(evidence).map_err(|error| {
        labeled_report_with_context(error, format!("序列化 compile evidence 失败 {}", output_path.display()), "VON 序列化失败位置")
    })?;
    fs::create_dir_all(output_dir).into_diagnostic().wrap_err_with(|| format!("创建输出目录失败 {}", output_dir.display()))?;
    fs::write(&output_path, content).into_diagnostic().wrap_err_with(|| format!("写入 compile evidence 失败 {}", output_path.display()))?;
    Ok(())
}

fn wasm_package_kind_for_manifest(artifact_kind: ProjectArtifactKind) -> nyar_emitter::nyar_backend_wasi::WasmPackageKind {
    match artifact_kind {
        ProjectArtifactKind::Library => nyar_emitter::nyar_backend_wasi::WasmPackageKind::Library,
        ProjectArtifactKind::Binary => nyar_emitter::nyar_backend_wasi::WasmPackageKind::Binary,
    }
}

pub(super) fn print_artifacts(output_dir: &Path, artifacts: &ArtifactSet) {
    for artifact in &artifacts.artifacts {
        let artifact_path = output_dir.join(&artifact.path);
        if !artifact_path.is_file() {
            println!("artifact: missing {}", artifact.path);
            continue;
        }
        println!("artifact: {}", artifact_path.display());
    }
}

fn write_execution_manifest(
    plan: &legion_workspace::planner::BuildPlan,
    specs: &[DriverRunContract],
    evidence: &rusty_valkyrie::CompilerCompileEvidence,
) -> Result<()> {
    let contracts = specs
        .iter()
        .map(|spec| RunContract {
            logical_entry: spec.logical_entry.clone(),
            physical_entry: spec.physical_entry.clone(),
            invocation: spec.invocation.clone(),
            validate: spec.validate.clone(),
        })
        .collect::<Vec<_>>();
    ExecutionManifest::from_build_plan(plan, &contracts, evidence)?.write_to_output_dir(&plan.output_dir)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HostSelectionEntry {
    contract: String,
    provider: String,
    source_file: String,
    line: usize,
}

fn write_host_selection_spec(output_dir: &Path, providers: &[legion_workspace::planner::PlannedHostProvider]) -> Result<()> {
    let output_path = output_dir.join("host-selection.txt");
    let entries: Vec<HostSelectionEntry> = providers
        .iter()
        .map(|item| HostSelectionEntry {
            contract: item.contract.clone(),
            provider: item.symbol.clone(),
            source_file: item.source_file.display().to_string(),
            line: item.line,
        })
        .collect();
    let content = write_von_indented(&entries).map_err(|error| {
        labeled_report_with_context(error, format!("序列化 host 选择结果失败 {}", output_path.display()), "VON 序列化失败位置")
    })?;
    fs::create_dir_all(output_dir).into_diagnostic().map_err(|error| error.wrap_err(format!("创建输出目录失败 {}", output_dir.display())))?;
    fs::write(&output_path, content)
        .into_diagnostic()
        .map_err(|error| error.wrap_err(format!("写入 host 选择结果失败 {}", output_path.display())))
}

fn remove_host_selection_spec(output_dir: &Path) -> Result<()> {
    let output_path = output_dir.join("host-selection.txt");
    if output_path.exists() {
        fs::remove_file(&output_path)
            .into_diagnostic()
            .map_err(|error| error.wrap_err(format!("删除 host 选择结果失败 {}", output_path.display())))?;
    }
    Ok(())
}
#[cfg(test)]
#[path = "../../../tests/path_wired/cmds_build_mod.rs"]
mod cmds_build_mod_tests;
