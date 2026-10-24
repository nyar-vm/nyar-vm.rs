use legion_workspace::{
    labeled_report_with_context,
    manifest::RunnerBinding,
    planner::{BuildPlan, BuildRequest, WorkspaceResolver},
    source_snapshot::compile_source_snapshot,
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use clap::Args;
use miette::{IntoDiagnostic, Result, miette};
use nyar_runner::{RuntimeContract as InterpreterRuntimeContract, RuntimeFamily as InterpreterRuntimeFamily};
use rusty_valkyrie::{CanonicalAbi, CanonicalTarget, RunnerFamily};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cmds::build::{BuildArgs, compile_arch, run as run_build};

mod artifact_selection;
pub(crate) use artifact_selection::select_artifact;

const EXECUTION_MANIFEST_FILE_NAME: &str = "run-contracts.txt";
const HOST_SELECTION_FILE_NAME: &str = "host-selection.txt";
/// 编译计划快照文件名，由构建流程写入，不属于交付产物。
const COMPILE_PLAN_SNAPSHOT_FILE_NAME: &str = "compile-plan.txt";
/// 后端执行请求快照文件名，由构建流程写入，不属于交付产物。
const BACKEND_REQUEST_SNAPSHOT_FILE_NAME: &str = "backend-request.txt";
/// 后端执行结果快照文件名，由构建流程写入，不属于交付产物。
const BACKEND_RESULT_SNAPSHOT_FILE_NAME: &str = "backend-result.txt";
const COMPILE_EVIDENCE_FILE_NAME: &str = "compile-evidence.txt";
/// execution manifest 的 schema 版本；未正式发布前统一为 `0`（v0 基线），
/// 旧 schema 的产物一律判为不新鲜并重建。
const EXECUTION_MANIFEST_SCHEMA_VERSION: u32 = 0;

/// `legion run` 的命令参数。
#[derive(Debug, Clone, Args)]
pub struct RunArgs {
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
    /// 覆盖 runner 命令，例如 `clr=C:\dotnet\dotnet.exe`。
    #[arg(long = "runner", value_name = "target=command")]
    pub runner: Vec<String>,
    /// 直接指定产物路径，跳过默认探测。
    #[arg(long, value_name = "artifact")]
    pub artifact: Option<PathBuf>,
    /// 只打印即将执行的命令，不真正启动。
    #[arg(long)]
    pub dry_run: bool,
    /// 输出调试用构建副产物。
    #[arg(long, default_value_t = false)]
    pub debug_artifacts: bool,
    /// 进程级工作负载意图 JSON（仅 `ovm`：转发给 `odin-vm --workload-json`）。
    #[arg(long = "workload-json", value_name = "json")]
    pub workload_json: Option<String>,
    /// 工作负载意图 JSON 文件路径（仅 `ovm`：转发给 `odin-vm --workload-file`）。
    #[arg(long = "workload-file", value_name = "path")]
    pub workload_file: Option<PathBuf>,
}

/// 一条可执行运行契约：逻辑入口、物理入口及调用/校验命令模板。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunContract {
    /// 逻辑入口符号名。
    pub logical_entry: String,
    /// 物理入口（产物中的文件名或符号）。
    pub physical_entry: String,
    /// 调用入口的命令模板。
    pub invocation: String,
    /// 校验入口可用的命令模板。
    pub validate: String,
}

/// Compiler 成功路径证据摘要（语义闭包 / 规范表面 / 宿主提供者绑定哈希）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompileEvidenceDigest {
    /// 语义闭包哈希。
    pub semantic_closure_hash: String,
    /// 规范表面哈希。
    pub canonical_surface_hash: String,
    /// 宿主提供者绑定哈希。
    pub host_provider_binding_hash: String,
    /// 独立生成源码摘要；无生成输入时为空串。
    #[serde(default)]
    pub generated_source_hash: String,
}

impl From<&rusty_valkyrie::CompilerCompileEvidence> for CompileEvidenceDigest {
    fn from(evidence: &rusty_valkyrie::CompilerCompileEvidence) -> Self {
        Self {
            semantic_closure_hash: evidence.semantic_closure_hash.clone(),
            canonical_surface_hash: evidence.canonical_surface_hash.clone(),
            host_provider_binding_hash: evidence.host_provider_binding_hash.clone(),
            generated_source_hash: evidence.generated_source_hash.clone(),
        }
    }
}

/// execution manifest：记录构建输入/产物摘要与运行契约，供 `run` 判断产物是否新鲜。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionManifest {
    /// manifest schema 版本。
    pub schema_version: u32,
    /// 语义 identity 合同版本（旧产物在漂移时失效）。
    pub identity_schema_version: u32,
    /// Semantic MIR 合同版本。
    pub mir_contract_version: u32,
    /// RepresentationPlan / layout 合同版本。
    pub layout_plan_version: u32,
    /// `.nyar` / `.legion` bytecode 格式版本。
    pub bytecode_format_version: u32,
    /// 项目名。
    pub project_name: String,
    /// 编译目标平台。
    pub target: String,
    /// 参与构建的输入文件摘要。
    pub inputs: Vec<ExecutionInputDigest>,
    /// 落盘产物摘要。
    pub artifacts: Vec<ExecutionArtifactDigest>,
    /// 该项目可用的运行契约列表。
    pub run_contracts: Vec<RunContract>,
    /// Compiler 成功路径的可复现证据（schema v3+）。
    pub compile_evidence: Option<CompileEvidenceDigest>,
}

/// 单个构建输入的摘要（角色 + 相对路径 + 哈希）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionInputDigest {
    /// 输入角色：`source` / `project-manifest` / `workspace-manifest`。
    pub role: String,
    /// 相对 workspace 根的归一化路径。
    pub path: String,
    /// 文件内容 SHA-256。
    pub hash: String,
}

/// 单个落盘产物的摘要（相对路径 + 哈希）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionArtifactDigest {
    /// 相对输出目录的路径。
    pub path: String,
    /// 产物内容 SHA-256。
    pub hash: String,
}

impl ExecutionManifest {
    /// 将 execution manifest 写入输出目录中的 `run-contracts.txt`。
    pub fn write_to_output_dir(&self, output_dir: &Path) -> Result<()> {
        let manifest_path = output_dir.join(EXECUTION_MANIFEST_FILE_NAME);
        let content = crate::write_von_indented(self).map_err(|error| {
            labeled_report_with_context(error, format!("序列化 execution manifest 失败 {}", manifest_path.display()), "VON 序列化失败位置")
        })?;
        fs::write(&manifest_path, content)
            .into_diagnostic()
            .map_err(|error| error.wrap_err(format!("写入 execution manifest 失败 {}", manifest_path.display())))?;

        Ok(())
    }

    /// 从输出目录读取 execution manifest。
    pub fn read_from_output_dir(output_dir: &Path) -> Result<Option<Self>> {
        Self::read_from_path(&output_dir.join(EXECUTION_MANIFEST_FILE_NAME))
    }

    /// 从指定路径读取 execution manifest。
    pub fn read_from_path(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let source = fs::read_to_string(path)
            .into_diagnostic()
            .map_err(|error| error.wrap_err(format!("failed to read execution manifest '{}'", path.display())))?;
        crate::parse_von::<Self>(&source).map(Some).map_err(|error| {
            labeled_report_with_context(error, format!("failed to parse execution manifest '{}'", path.display()), "VON 解析失败位置")
        })
    }

    /// 删除输出目录中的 execution manifest。
    pub fn remove_from_output_dir(output_dir: &Path) -> Result<()> {
        let manifest_path = output_dir.join(EXECUTION_MANIFEST_FILE_NAME);
        if manifest_path.exists() {
            fs::remove_file(&manifest_path)
                .into_diagnostic()
                .map_err(|error| error.wrap_err(format!("删除 execution manifest 失败 {}", manifest_path.display())))?;
        }
        Ok(())
    }

    /// 基于当前 `BuildPlan`、运行契约与编译证据生成 execution manifest。
    pub fn from_build_plan(plan: &BuildPlan, contracts: &[RunContract], evidence: &rusty_valkyrie::CompilerCompileEvidence) -> Result<Self> {
        Ok(Self {
            schema_version: EXECUTION_MANIFEST_SCHEMA_VERSION,
            identity_schema_version: nyar_types::IDENTITY_SCHEMA_VERSION,
            mir_contract_version: nyar_types::MIR_CONTRACT_VERSION,
            layout_plan_version: nyar_types::LAYOUT_PLAN_VERSION,
            bytecode_format_version: odin_bytecode::BYTECODE_FORMAT_VERSION,
            project_name: plan.project.name.clone(),
            target: plan.project.build_target.target.to_string(),
            inputs: collect_execution_input_digests(plan)?,
            artifacts: collect_execution_artifact_digests(&plan.output_dir)?,
            run_contracts: contracts.to_vec(),
            compile_evidence: Some(CompileEvidenceDigest::from(evidence)),
        })
    }

    /// 校验当前 execution manifest 是否仍与 `BuildPlan` 及产物目录匹配。
    pub fn is_fresh_for_plan(&self, plan: &BuildPlan) -> Result<bool> {
        if self.schema_version != EXECUTION_MANIFEST_SCHEMA_VERSION {
            return Ok(false);
        }
        if self.identity_schema_version != nyar_types::IDENTITY_SCHEMA_VERSION
            || self.mir_contract_version != nyar_types::MIR_CONTRACT_VERSION
            || self.layout_plan_version != nyar_types::LAYOUT_PLAN_VERSION
            || self.bytecode_format_version != odin_bytecode::BYTECODE_FORMAT_VERSION
        {
            return Ok(false);
        }
        if self.project_name != plan.project.name {
            return Ok(false);
        }
        if self.target != plan.project.build_target.target.to_string() {
            return Ok(false);
        }
        if self.inputs != collect_execution_input_digests(plan)? {
            return Ok(false);
        }
        if self.run_contracts.is_empty() || self.artifacts.is_empty() {
            return Ok(false);
        }

        for artifact in &self.artifacts {
            let artifact_path = plan.output_dir.join(PathBuf::from(&artifact.path));
            if !artifact_path.exists() {
                return Ok(false);
            }
            if hash_file_sha256(&artifact_path)? != artifact.hash {
                return Ok(false);
            }
        }

        let Some(stored) = &self.compile_evidence
        else {
            return Ok(false);
        };
        let expected = expected_resolver_compile_evidence(plan)?;
        if stored.semantic_closure_hash != expected.semantic_closure_hash
            || stored.host_provider_binding_hash != expected.host_provider_binding_hash
            || stored.generated_source_hash != expected.generated_source_hash
        {
            return Ok(false);
        }

        let Some(file_evidence) = read_compile_evidence_digest(&plan.output_dir)?
        else {
            return Ok(false);
        };
        if file_evidence != *stored {
            return Ok(false);
        }

        Ok(true)
    }
}

fn read_compile_evidence_digest(output_dir: &Path) -> Result<Option<CompileEvidenceDigest>> {
    let path = output_dir.join(COMPILE_EVIDENCE_FILE_NAME);
    if !path.exists() {
        return Ok(None);
    }
    let source = fs::read_to_string(&path)
        .into_diagnostic()
        .map_err(|error| error.wrap_err(format!("读取 compile evidence 失败 {}", path.display())))?;
    let evidence = crate::parse_von::<rusty_valkyrie::CompilerCompileEvidence>(&source)
        .map_err(|error| labeled_report_with_context(error, format!("解析 compile evidence 失败 {}", path.display()), "VON 解析失败位置"))?;
    Ok(Some(CompileEvidenceDigest::from(&evidence)))
}

fn expected_resolver_compile_evidence(plan: &BuildPlan) -> Result<CompileEvidenceDigest> {
    let groups = compile_source_snapshot(&plan.project.semantic_source_groups)?;
    let host_bindings = plan
        .project
        .selected_host_providers
        .iter()
        .map(|provider| {
            let contract =
                provider.contract_key.clone().ok_or_else(|| miette!("选中的 host provider `{}` 缺少契约声明身份", provider.symbol))?;
            Ok(rusty_valkyrie::CompilerHostProviderBinding { contract, provider: provider.key.clone() })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(expected_resolver_compile_evidence_from_groups(&groups, &host_bindings, compile_arch(plan)))
}

/// 与 build 阶段共用同一架构键，保证 compile evidence 可比较。
fn expected_resolver_compile_evidence_from_groups(
    groups: &[nyar_types::compiler_input::CompilerSourceGroup],
    host_bindings: &[rusty_valkyrie::CompilerHostProviderBinding],
    arch: &str,
) -> CompileEvidenceDigest {
    CompileEvidenceDigest {
        semantic_closure_hash: rusty_valkyrie::semantic_closure_hash_for_groups(groups, arch),
        canonical_surface_hash: String::new(),
        host_provider_binding_hash: rusty_valkyrie::host_provider_binding_hash_for_bindings(host_bindings),
        generated_source_hash: String::new(),
    }
}

impl RunContract {
    /// 从输出目录读取主运行契约。
    pub fn read_from_output_dir(output_dir: &Path) -> Result<Option<Self>> {
        Ok(Self::read_all_from_output_dir(output_dir)?.into_iter().next())
    }

    /// 从输出目录读取运行契约列表。
    pub fn read_all_from_output_dir(output_dir: &Path) -> Result<Vec<Self>> {
        Ok(ExecutionManifest::read_from_output_dir(output_dir)?.map_or_else(Vec::new, |manifest| manifest.run_contracts))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunnerTemplate {
    target: RunnerFamily,
    command: String,
    args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunCommand {
    target: RunnerFamily,
    artifact: PathBuf,
    command: String,
    args: Vec<String>,
}

/// 执行 `legion run`。
pub fn run(args: &RunArgs) -> Result<ExitCode> {
    let workspace = if args.workspace {
        WorkspaceResolver::discover(&args.project_dir)?
    }
    else {
        WorkspaceResolver::discover_for_project(&args.project_dir)?
    };
    let request = BuildRequest { project_dir: args.project_dir.clone(), target: args.target.clone(), output_dir: args.output_dir.clone() };
    let plan = workspace.build_plan(&request)?;

    let execution_manifest = ensure_execution_manifest(args, &plan)?;
    let mut command = plan_run_command(
        &workspace,
        &plan.output_dir,
        &plan.project.build_target.target,
        &args.runner,
        &execution_manifest.run_contracts,
        args.artifact.as_deref(),
    )?;
    append_ovm_workload_args(&mut command, args)?;

    println!("project: {}", plan.project.name);
    println!("target: {}", plan.project.build_target.target);
    println!("output: {}", crate::cmds::path_for_cli_log(&plan.output_dir));
    println!("mode: workspace");
    println!("artifact: {}", command.artifact.display());
    println!("runner: {}", command.command);
    println!("args: {}", shell_join(&command.args));

    if args.dry_run {
        println!("run status: dry-run");
        return Ok(ExitCode::SUCCESS);
    }

    let status = Command::new(&command.command)
        .args(command.args.iter().map(OsString::from))
        .current_dir(&plan.project.manifest_dir)
        .status()
        .into_diagnostic()
        .map_err(|error| error.wrap_err(format!("failed to start runner '{}'", command.command)))?;

    Ok(exit_code_from_status(status.code()))
}

/// 将 legion 侧工作负载意图参数转发给 `odin-vm` runner。
fn append_ovm_workload_args(command: &mut RunCommand, args: &RunArgs) -> Result<()> {
    if args.workload_json.is_none() && args.workload_file.is_none() {
        return Ok(());
    }
    if command.target != RunnerFamily::Ovm {
        return Err(miette!("--workload-json/--workload-file are only supported for target ovm"));
    }
    if let Some(json) = &args.workload_json {
        command.args.push("--workload-json".to_string());
        command.args.push(json.clone());
    }
    if let Some(path) = &args.workload_file {
        command.args.push("--workload-file".to_string());
        command.args.push(strip_verbatim_prefix(path.to_string_lossy().as_ref()).to_owned());
    }
    Ok(())
}

fn plan_run_command(
    workspace: &WorkspaceResolver,
    output_dir: &Path,
    canonical_target: &CanonicalTarget,
    cli_runner_overrides: &[String],
    run_contracts: &[RunContract],
    artifact_override: Option<&Path>,
) -> Result<RunCommand> {
    let runner_target = runner_target_for(canonical_target);
    let (artifact, contract_index) =
        select_artifact(output_dir, run_contracts.iter().map(|contract| contract.physical_entry.as_str()), artifact_override)
            .map_err(|error| miette!("{error}"))?;
    let run_contract = &run_contracts[contract_index];
    let runner = resolve_runner(workspace, canonical_target, runner_target, cli_runner_overrides, Some(run_contract))?;
    let placeholders = build_placeholders(output_dir, &artifact, runner_target, Some(run_contract))?;
    let command = expand_placeholders(&runner.command, &placeholders);
    let args = expand_runner_args(&runner.args, &placeholders);

    Ok(RunCommand { target: runner_target, artifact, command, args })
}

fn ensure_execution_manifest(args: &RunArgs, plan: &BuildPlan) -> Result<ExecutionManifest> {
    if let Some(manifest) = ExecutionManifest::read_from_output_dir(&plan.output_dir)? {
        if manifest.is_fresh_for_plan(plan)? {
            println!("build status: reused");
            return Ok(manifest);
        }
        println!("build status: stale");
    }
    else {
        println!("build status: missing");
    }

    let build_status = run_build(&BuildArgs {
        project_dir: args.project_dir.clone(),
        target: args.target.clone(),
        output_dir: args.output_dir.clone(),
        workspace: args.workspace,
        debug_artifacts: args.debug_artifacts,
    })?;
    if build_status != ExitCode::SUCCESS {
        return Err(miette!("build returned non-zero status"));
    }

    let manifest = ExecutionManifest::read_from_output_dir(&plan.output_dir)?
        .ok_or_else(|| miette!("missing execution manifest after build: {}", plan.output_dir.join(EXECUTION_MANIFEST_FILE_NAME).display()))?;
    if !manifest.is_fresh_for_plan(plan)? {
        return Err(miette!("execution manifest is still stale after build"));
    }

    Ok(manifest)
}

fn resolve_runner(
    workspace: &WorkspaceResolver,
    canonical_target: &CanonicalTarget,
    runner_target: RunnerFamily,
    cli_runner_overrides: &[String],
    run_contract: Option<&RunContract>,
) -> Result<RunnerTemplate> {
    let mut default_template = default_runner_template(runner_target, run_contract);
    if runner_target == RunnerFamily::Wasi && is_wasip3_target(canonical_target) {
        inject_wasmtime_p3_flag(&mut default_template.args);
    }

    if let Some(command) = parse_runner_overrides(cli_runner_overrides)?.remove(&runner_target) {
        let mut template = default_template.clone();
        template.command = command;
        return Ok(template);
    }

    if let Some(workspace_manifest) = &workspace.workspace_manifest {
        if let Some(binding) = workspace_manifest.runner.iter().find(|binding| runner_binding_matches(binding, runner_target, canonical_target))
        {
            let mut template = RunnerTemplate { target: runner_target, command: binding.command.clone(), args: binding.args.clone() };
            if runner_target == RunnerFamily::Wasi && is_wasip3_target(canonical_target) {
                inject_wasmtime_p3_flag(&mut template.args);
            }
            return Ok(template);
        }
    }

    if let Ok(command) = std::env::var(format!("LEGION_RUNNER_{}", runner_target.as_str().to_ascii_uppercase())) {
        if !command.trim().is_empty() {
            let mut template = default_template.clone();
            template.command = command;
            return Ok(template);
        }
    }

    Ok(default_template)
}

fn is_wasip3_target(canonical_target: &CanonicalTarget) -> bool {
    matches!(canonical_target.abi, Some(CanonicalAbi::WasiP3))
        || canonical_target.to_string().contains("wasip3")
        || canonical_target.to_profile(None).capability_tags.iter().any(|tag| tag == "wasip3")
}

fn inject_wasmtime_p3_flag(args: &mut Vec<String>) {
    if args.windows(2).any(|pair| pair[0] == "-S" && pair[1] == "p3") {
        return;
    }
    let insert_at = args.len().saturating_sub(1);
    args.insert(insert_at, "-S".to_string());
    args.insert(insert_at + 1, "p3".to_string());
}

fn parse_runner_overrides(values: &[String]) -> Result<BTreeMap<RunnerFamily, String>> {
    let mut overrides = BTreeMap::new();
    for item in values {
        let Some((target, command)) = item.split_once('=')
        else {
            return Err(miette!("invalid runner override '{}': expected target=command", item));
        };
        let family = target.trim().parse::<RunnerFamily>().map_err(|error| miette!("invalid runner override '{}': {}", item, error))?;
        overrides.insert(family, command.trim().to_string());
    }
    Ok(overrides)
}

fn runner_binding_matches(binding: &RunnerBinding, runner_target: RunnerFamily, canonical_target: &CanonicalTarget) -> bool {
    binding.target.matches(runner_target, canonical_target)
}

fn default_runner_template(target: RunnerFamily, run_contract: Option<&RunContract>) -> RunnerTemplate {
    let template = runtime_family_for(target).default_template(interpreter_runtime_contract(run_contract));
    RunnerTemplate { target, command: template.command, args: template.args }
}

fn runner_target_for(canonical_target: &CanonicalTarget) -> RunnerFamily {
    canonical_target.to_profile(None).runner_family()
}

fn runtime_family_for(target: RunnerFamily) -> InterpreterRuntimeFamily {
    match target {
        RunnerFamily::Clr => InterpreterRuntimeFamily::Clr,
        RunnerFamily::Jvm => InterpreterRuntimeFamily::Jvm,
        RunnerFamily::Node => InterpreterRuntimeFamily::Node,
        RunnerFamily::Windows => InterpreterRuntimeFamily::Windows,
        RunnerFamily::Wasi => InterpreterRuntimeFamily::Wasi,
        RunnerFamily::Ovm => InterpreterRuntimeFamily::Ovm,
    }
}

fn interpreter_runtime_contract(run_contract: Option<&RunContract>) -> Option<InterpreterRuntimeContract<'_>> {
    run_contract.map(|contract| InterpreterRuntimeContract {
        logical_entry: (!contract.logical_entry.is_empty()).then_some(contract.logical_entry.as_str()),
        physical_entry: (!contract.physical_entry.is_empty()).then_some(contract.physical_entry.as_str()),
        wasi_p3: contract.validate.contains("-S p3") || contract.validate.contains(" p3"),
    })
}

fn build_placeholders(
    output_dir: &Path,
    artifact: &Path,
    runner_target: RunnerFamily,
    run_contract: Option<&RunContract>,
) -> Result<BTreeMap<&'static str, String>> {
    let mut values = BTreeMap::new();
    let artifact_text = strip_verbatim_prefix(artifact.to_string_lossy().as_ref()).to_owned();
    values.insert("artifact", artifact_text.clone());

    let classpath = if artifact.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("jar")) {
        artifact_text.clone()
    }
    else {
        strip_verbatim_prefix(output_dir.to_string_lossy().as_ref()).to_owned()
    };
    values.insert("classpath", classpath);

    // `{entry}` is the callable symbol. Nyar VM / JVM keep the module file in `{artifact}`
    // (or classpath) and must not reuse `physical_entry` filenames here.
    let entry = match run_contract {
        Some(contract) if matches!(runner_target, RunnerFamily::Jvm | RunnerFamily::Ovm) && !contract.logical_entry.is_empty() => {
            contract.logical_entry.clone()
        }
        Some(contract) if !contract.physical_entry.is_empty() => contract.physical_entry.clone(),
        Some(contract) if !contract.logical_entry.is_empty() => contract.logical_entry.clone(),
        _ if runner_target == RunnerFamily::Jvm => artifact
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(ToOwned::to_owned)
            .ok_or_else(|| miette!("cannot infer JVM entry from {}", artifact.display()))?,
        _ => artifact.file_stem().and_then(|stem| stem.to_str()).unwrap_or_default().to_string(),
    };
    values.insert("entry", entry);
    Ok(values)
}

fn expand_runner_args(args: &[String], placeholders: &BTreeMap<&'static str, String>) -> Vec<String> {
    args.iter().map(|value| expand_placeholders(value, placeholders)).collect()
}

fn expand_placeholders(template: &str, placeholders: &BTreeMap<&'static str, String>) -> String {
    placeholders.iter().fold(template.to_string(), |current, (key, value)| current.replace(&format!("{{{}}}", key), value))
}

fn collect_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Err(miette!("output directory does not exist: {}", dir.display()));
    }

    let mut files = Vec::new();
    for entry in fs::read_dir(dir).into_diagnostic().map_err(|error| error.wrap_err(format!("failed to list '{}'", dir.display())))? {
        let entry = entry.into_diagnostic().map_err(|error| error.wrap_err(format!("failed to read '{}'", dir.display())))?;
        let path = entry.path();
        if path.is_file() {
            files.push(path);
        }
    }
    Ok(files)
}

fn shell_join(args: &[String]) -> String {
    if args.is_empty() {
        return String::new();
    }

    args.iter().map(|value| if value.contains(' ') { format!("\"{}\"", value) } else { value.clone() }).collect::<Vec<_>>().join(" ")
}

fn exit_code_from_status(code: Option<i32>) -> ExitCode {
    match code {
        Some(value) if (0..=255).contains(&value) => ExitCode::from(value as u8),
        Some(_) => ExitCode::from(1),
        None => ExitCode::from(1),
    }
}

/// 移除 Windows extended-length path 前缀 `\\?\`，避免 Node/wasmtime 等外部工具无法解析。
fn strip_verbatim_prefix(path: &str) -> &str {
    path.strip_prefix(r"\\?\").unwrap_or(path)
}

fn collect_execution_input_digests(plan: &BuildPlan) -> Result<Vec<ExecutionInputDigest>> {
    let mut inputs = Vec::new();
    for source_path in &plan.project.source_files {
        inputs.push(ExecutionInputDigest {
            role: "source".to_string(),
            path: workspace_relative_path_text(&plan.workspace_root, source_path),
            hash: hash_file_sha256(source_path)?,
        });
    }

    inputs.push(ExecutionInputDigest {
        role: "project-manifest".to_string(),
        path: workspace_relative_path_text(&plan.workspace_root, &plan.project.manifest_path),
        hash: hash_file_sha256(&plan.project.manifest_path)?,
    });

    let workspace_manifest_path = plan.workspace_root.join("legions.von");
    if workspace_manifest_path.exists() {
        inputs.push(ExecutionInputDigest {
            role: "workspace-manifest".to_string(),
            path: workspace_relative_path_text(&plan.workspace_root, &workspace_manifest_path),
            hash: hash_file_sha256(&workspace_manifest_path)?,
        });
    }

    inputs.sort_by(|left, right| left.role.cmp(&right.role).then(left.path.cmp(&right.path)));
    Ok(inputs)
}

fn collect_execution_artifact_digests(output_dir: &Path) -> Result<Vec<ExecutionArtifactDigest>> {
    let mut files = collect_files(output_dir)?;
    files.retain(|path| !is_execution_metadata_file(path));
    files.sort();

    files
        .into_iter()
        .map(|path| Ok(ExecutionArtifactDigest { path: relative_output_path(output_dir, &path), hash: hash_file_sha256(&path)? }))
        .collect()
}

fn is_execution_metadata_file(path: &Path) -> bool {
    path.file_name().and_then(|value| value.to_str()).is_some_and(|value| {
        value.eq_ignore_ascii_case(EXECUTION_MANIFEST_FILE_NAME)
            || value.eq_ignore_ascii_case(HOST_SELECTION_FILE_NAME)
            || value.eq_ignore_ascii_case(COMPILE_PLAN_SNAPSHOT_FILE_NAME)
            || value.eq_ignore_ascii_case(BACKEND_REQUEST_SNAPSHOT_FILE_NAME)
            || value.eq_ignore_ascii_case(BACKEND_RESULT_SNAPSHOT_FILE_NAME)
            || value.eq_ignore_ascii_case(COMPILE_EVIDENCE_FILE_NAME)
    })
}

fn relative_output_path(output_dir: &Path, path: &Path) -> String {
    path.strip_prefix(output_dir).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

fn normalized_path_text(path: &Path) -> String {
    strip_verbatim_prefix(path.to_string_lossy().as_ref()).replace('\\', "/")
}

/// 写入 execution manifest 的路径必须相对 workspace 根，禁止落盘本机绝对路径。
fn workspace_relative_path_text(workspace_root: &Path, path: &Path) -> String {
    let absolute_owned = path.to_string_lossy().into_owned();
    let root_owned = workspace_root.to_string_lossy().into_owned();
    let absolute = strip_verbatim_prefix(&absolute_owned);
    let root = strip_verbatim_prefix(&root_owned);
    let absolute_norm = absolute.replace('/', "\\");
    let root_norm = root.trim_end_matches(['/', '\\']).replace('/', "\\");
    if let Some(rel) = absolute_norm.strip_prefix(&root_norm) {
        let trimmed = rel.trim_start_matches(['/', '\\']);
        if !trimmed.is_empty() {
            return trimmed.replace('\\', "/");
        }
    }
    if let Ok(rel) = path.strip_prefix(workspace_root) {
        return rel.to_string_lossy().replace('\\', "/");
    }
    // 无法相对化时只保留文件名，避免把绝对路径写进产物。
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| normalized_path_text(path))
}

fn hash_file_sha256(path: &Path) -> Result<String> {
    let bytes = fs::read(path).into_diagnostic().map_err(|error| error.wrap_err(format!("failed to read '{}'", path.display())))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hex_encode(&hasher.finalize()))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(&mut text, "{byte:02x}");
    }
    text
}
#[cfg(test)]
#[path = "../../../tests/path_wired/cmds_run_mod_workload_forward_tests.rs"]
mod cmds_run_mod_workload_forward_tests_tests;
