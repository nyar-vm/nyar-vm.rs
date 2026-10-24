use legion::{
    CanonicalTarget,
    cmds::{
        build::{BuildArgs, run as run_build},
        run::RunContract,
    },
};
use nyar::{assert_or_regenerate_yaml_sidecar, collect_fixture_cases_with_extensions, resolve_runtime_fixture_targets};
use nyar_runner::{RuntimeContract as RunnerRuntimeContract, RuntimeFamily};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    sync::{Mutex, OnceLock},
};

use crate::support::{
    create_local_package_project_with_manifest, create_nested_workspace_member_project_with_manifest, create_script_project_with_manifest,
    create_smoke_project_with_manifest,
};

use super::runtime_fixture::regenerate_enabled;

const DEFAULT_RUN_FIXTURE_TARGETS: [RunFixtureTarget; 4] =
    [RunFixtureTarget::Clr, RunFixtureTarget::Jvm, RunFixtureTarget::Node, RunFixtureTarget::Wasi];
const DEFAULT_RUN_FIXTURE_TARGET_NAMES: [&str; 4] =
    ["clr-microsoft-unknown-managed", "jvm-openjdk-unknown-managed", "wasm32-node-unknown-wasm", "wasm32-unknown-wasi-wasi"];
static WASI_RUNTIME_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunFixtureProjectMode {
    Script,
    Package,
    WorkspaceMember,
    NestedWorkspaceMember,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunFixtureContext {
    pub targets: Vec<String>,
    pub project_mode: RunFixtureProjectMode,
    pub manifest: Option<String>,
    pub project_name: Option<String>,
}

impl RunFixtureContext {
    pub fn new(targets: &[&str]) -> Self {
        Self {
            targets: targets.iter().map(|target| (*target).to_string()).collect(),
            project_mode: RunFixtureProjectMode::WorkspaceMember,
            manifest: None,
            project_name: None,
        }
    }

    pub fn script(mut self) -> Self {
        self.project_mode = RunFixtureProjectMode::Script;
        self
    }

    pub fn local_package(mut self) -> Self {
        self.project_mode = RunFixtureProjectMode::Package;
        self
    }

    pub fn package(mut self) -> Self {
        self.project_mode = RunFixtureProjectMode::Package;
        self
    }

    pub fn nested_workspace_member(mut self) -> Self {
        self.project_mode = RunFixtureProjectMode::NestedWorkspaceMember;
        self
    }

    pub fn with_manifest(mut self, manifest: impl Into<String>) -> Self {
        self.manifest = Some(manifest.into());
        self
    }

    pub fn with_project_name(mut self, project_name: impl Into<String>) -> Self {
        self.project_name = Some(project_name.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RunDelegationFixtureSpec {
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub expect: BTreeMap<String, RunDelegationFixtureResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RunDelegationFixtureResult {
    #[serde(default)]
    pub build_success: bool,
    #[serde(default)]
    pub dry_run_success: bool,
    #[serde(default)]
    pub run_success: bool,
    #[serde(default)]
    pub run_contracts: Vec<RunContract>,
    #[serde(default)]
    pub per_artifact: BTreeMap<String, RunArtifactExecutionResult>,
    #[serde(default)]
    pub artifacts: Vec<String>,
    #[serde(default)]
    pub compile_errors: Vec<String>,
    #[serde(default)]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RunArtifactExecutionResult {
    #[serde(default)]
    pub run_success: bool,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub stdout: Vec<String>,
    #[serde(default)]
    pub stderr: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunFixtureTarget {
    Clr,
    Jvm,
    Node,
    Wasi,
}

impl RunFixtureTarget {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "clr" | "clr-microsoft-unknown-managed" => Some(Self::Clr),
            "jvm" | "jvm-openjdk-unknown-managed" => Some(Self::Jvm),
            "node" | "wasm32-node-unknown-wasm" => Some(Self::Node),
            "wasi" | "wasm32-unknown-wasi-wasi" => Some(Self::Wasi),
            _ => None,
        }
    }

    fn fixture_key(self) -> String {
        self.canonical_target().as_canonical_str()
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Clr => "clr",
            Self::Jvm => "jvm",
            Self::Node => "node",
            Self::Wasi => "wasi",
        }
    }

    fn manifest_target(self) -> &'static str {
        match self {
            Self::Clr => "clr",
            Self::Jvm => "jvm-openjdk-unknown-managed",
            Self::Node => "node",
            Self::Wasi => "wasi",
        }
    }

    fn canonical_target(self) -> CanonicalTarget {
        match self {
            Self::Clr => CanonicalTarget::clr(),
            Self::Jvm => CanonicalTarget::parse("jvm-openjdk-unknown-managed").unwrap(),
            Self::Node => CanonicalTarget::parse("node").unwrap(),
            Self::Wasi => CanonicalTarget::parse("wasi").unwrap(),
        }
    }

    fn required_command(self) -> (&'static str, &'static str) {
        match self {
            Self::Clr => ("dotnet", "dotnet"),
            Self::Jvm => ("java", "java"),
            Self::Node => ("node", "node"),
            Self::Wasi => ("wasmtime", "wasmtime"),
        }
    }
}

pub fn collect_run_fixture_cases(root: &Path) -> Vec<PathBuf> {
    collect_fixture_cases_with_extensions(root, &["valkyrie"])
}

pub fn can_run_all_runtime_targets(fixtures: &[PathBuf]) -> bool {
    for target in fixture_targets(fixtures) {
        let (label, command) = target.required_command();
        if !command_exists(command) {
            eprintln!("skip run fixture: missing {}", label);
            return false;
        }
    }
    true
}

pub fn verify_run_fixture(fixture_path: &Path) {
    verify_run_fixture_with_context(fixture_path, None);
}

pub fn can_run_runtime_targets(targets: &[&str]) -> bool {
    parse_run_fixture_targets(targets).iter().all(|target| command_exists(target.required_command().1))
}

pub fn can_run_fixture_context(context: &RunFixtureContext) -> bool {
    context
        .targets
        .iter()
        .map(|target| parse_run_fixture_target(target, Path::new("<explicit-targets>")))
        .all(|target| command_exists(target.required_command().1))
}

pub fn verify_run_fixture_for_context(fixture_path: &Path, context: &RunFixtureContext) {
    verify_run_fixture_with_context(fixture_path, Some(context));
}

fn verify_run_fixture_with_context(fixture_path: &Path, explicit_context: Option<&RunFixtureContext>) {
    let source = fs::read_to_string(fixture_path)
        .unwrap_or_else(|error| panic!("failed to read run fixture '{}': {}", fixture_path.display(), error))
        .replace("\r\n", "\n");
    let project_name = explicit_context.and_then(|context| context.project_name.clone()).unwrap_or_else(|| fixture_project_name(fixture_path));
    let targets = explicit_context
        .map(|context| parse_run_fixture_target_strings(&context.targets))
        .unwrap_or_else(|| fixture_targets_for_path(fixture_path));
    let manifest = explicit_context.and_then(|context| context.manifest.clone()).unwrap_or_else(|| fixture_manifest(&project_name, &targets));
    let fixture = match explicit_context.map(|context| context.project_mode).unwrap_or(RunFixtureProjectMode::WorkspaceMember) {
        RunFixtureProjectMode::Script => create_script_project_with_manifest(&fixture_prefix(fixture_path), &manifest, &source),
        RunFixtureProjectMode::Package => create_local_package_project_with_manifest(&fixture_prefix(fixture_path), &manifest, &source),
        RunFixtureProjectMode::WorkspaceMember => create_smoke_project_with_manifest(&fixture_prefix(fixture_path), &manifest, &source),
        RunFixtureProjectMode::NestedWorkspaceMember => {
            create_nested_workspace_member_project_with_manifest(&fixture_prefix(fixture_path), &manifest, &source)
        }
    };

    let mut expect = BTreeMap::new();
    for target in targets.iter().copied() {
        expect.insert(target.fixture_key(), execute_run_fixture_target(&fixture.project_dir, target));
    }

    let observed = RunDelegationFixtureSpec { targets: targets.iter().map(|target| target.fixture_key()).collect(), expect };
    assert_or_regenerate_yaml_sidecar(fixture_path, &observed, regenerate_enabled());
}

fn fixture_project_name(fixture_path: &Path) -> String {
    let stem = fixture_path.file_stem().and_then(|value| value.to_str()).unwrap_or("run_fixture");
    sanitize_identifier(stem)
}

fn fixture_prefix(fixture_path: &Path) -> String {
    format!("legion-run-{}", fixture_project_name(fixture_path))
}

fn fixture_manifest(project_name: &str, targets: &[RunFixtureTarget]) -> String {
    let build = targets
        .iter()
        .map(|target| match target {
            RunFixtureTarget::Clr => r#"        {
            target: "clr",
            msil: true
        }"#
            .to_string(),
            _ => format!(
                r#"        {{
            target: "{}"
        }}"#,
                target.manifest_target()
            ),
        })
        .collect::<Vec<_>>()
        .join(",\n");

    format!(
        r#"{{
    name: "{project_name}",
    version: "0.1.0",
    dependencies: {{
        "std": false,
        "core": false
    }},
    build: [
{build}
    ]
}}
"#
    )
}

fn fixture_targets(fixtures: &[PathBuf]) -> Vec<RunFixtureTarget> {
    let mut resolved = Vec::new();
    for fixture_path in fixtures {
        for target in fixture_targets_for_path(fixture_path) {
            if !resolved.contains(&target) {
                resolved.push(target);
            }
        }
    }
    resolved
}

fn fixture_targets_for_path(fixture_path: &Path) -> Vec<RunFixtureTarget> {
    resolve_runtime_fixture_targets(fixture_path, &DEFAULT_RUN_FIXTURE_TARGET_NAMES)
        .into_iter()
        .map(|target| parse_run_fixture_target(&target, fixture_path))
        .collect()
}

fn parse_run_fixture_targets(targets: &[&str]) -> Vec<RunFixtureTarget> {
    targets.iter().map(|target| parse_run_fixture_target(target, Path::new("<explicit-targets>"))).collect()
}

fn parse_run_fixture_target_strings(targets: &[String]) -> Vec<RunFixtureTarget> {
    targets.iter().map(|target| parse_run_fixture_target(target, Path::new("<explicit-targets>"))).collect()
}

fn parse_run_fixture_target(target: &str, fixture_path: &Path) -> RunFixtureTarget {
    RunFixtureTarget::parse(target)
        .unwrap_or_else(|| panic!("run fixture '{}' contains unsupported target '{}'", fixture_path.display(), target))
}

fn execute_run_fixture_target(project_dir: &Path, target: RunFixtureTarget) -> RunDelegationFixtureResult {
    let output_dir = project_dir.join("dist").join(target.as_str());
    let build_status = match run_build(&BuildArgs {
        project_dir: project_dir.to_path_buf(),
        target: target.canonical_target(),
        output_dir: Some(output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    }) {
        Ok(status) => status,
        Err(error) => {
            return RunDelegationFixtureResult {
                artifacts: collect_output_entries(&output_dir),
                compile_errors: vec![format!("build failed for {}: {}", target.fixture_key(), error)],
                ..RunDelegationFixtureResult::default()
            };
        }
    };

    if build_status != ExitCode::SUCCESS {
        return RunDelegationFixtureResult {
            artifacts: collect_output_entries(&output_dir),
            compile_errors: vec![format!("build returned non-zero status for {}", target.fixture_key())],
            ..RunDelegationFixtureResult::default()
        };
    }

    let run_contracts = match RunContract::read_all_from_output_dir(&output_dir) {
        Ok(contracts) => contracts,
        Err(error) => {
            return RunDelegationFixtureResult {
                artifacts: collect_output_entries(&output_dir),
                build_success: true,
                errors: vec![error.to_string()],
                ..RunDelegationFixtureResult::default()
            };
        }
    };

    let output = match run_legion_dry_run(project_dir, target, &output_dir) {
        Ok(success) => success,
        Err(error) => {
            return RunDelegationFixtureResult {
                artifacts: collect_output_entries(&output_dir),
                build_success: true,
                run_contracts,
                errors: vec![format!("run command failed for {}: {}", target.fixture_key(), error)],
                ..RunDelegationFixtureResult::default()
            };
        }
    };

    let dry_run_success = output;
    if !dry_run_success {
        return RunDelegationFixtureResult {
            artifacts: collect_output_entries(&output_dir),
            build_success: true,
            dry_run_success: false,
            run_contracts,
            errors: vec![format!("dry run returned non-zero status for {}", target.fixture_key())],
            ..RunDelegationFixtureResult::default()
        };
    }

    let mut per_artifact = BTreeMap::new();
    let mut errors = Vec::new();
    let mut run_success = true;

    if run_contracts.is_empty() {
        run_success = false;
        errors.push(format!("missing run contracts for {}", target.fixture_key()));
    }

    for run_contract in &run_contracts {
        let artifact_key = artifact_key_for_contract(run_contract);
        match prepare_runtime_command(&output_dir, target, Some(run_contract)) {
            Ok(command) => match run_runtime_command(project_dir, &output_dir, target, &command) {
                Ok(output) => {
                    let artifact_success = output.status.success();
                    if !artifact_success {
                        run_success = false;
                    }
                    per_artifact.insert(
                        artifact_key,
                        RunArtifactExecutionResult {
                            run_success: artifact_success,
                            exit_code: output.status.code(),
                            stdout: normalize_output_lines(&output.stdout),
                            stderr: normalize_output_lines(&output.stderr),
                        },
                    );
                }
                Err(error) => {
                    run_success = false;
                    errors.push(format!("failed to execute runtime command for {}: {}", target.fixture_key(), error));
                    per_artifact.insert(
                        artifact_key,
                        RunArtifactExecutionResult { run_success: false, exit_code: None, stdout: Vec::new(), stderr: vec![error.to_string()] },
                    );
                }
            },
            Err(error) => {
                run_success = false;
                errors.push(format!("failed to prepare runtime command for {}: {}", target.fixture_key(), error));
                per_artifact.insert(
                    artifact_key,
                    RunArtifactExecutionResult { run_success: false, exit_code: None, stdout: Vec::new(), stderr: vec![error] },
                );
            }
        }
    }

    RunDelegationFixtureResult {
        artifacts: collect_output_entries(&output_dir),
        build_success: true,
        dry_run_success,
        run_success,
        run_contracts,
        per_artifact,
        compile_errors: Vec::new(),
        errors,
    }
}

fn artifact_key_for_contract(run_contract: &RunContract) -> String {
    if !run_contract.physical_entry.is_empty() {
        return run_contract.physical_entry.clone();
    }
    if !run_contract.logical_entry.is_empty() {
        return run_contract.logical_entry.clone();
    }
    "unknown-artifact".to_string()
}

fn sanitize_identifier(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            result.push(ch.to_ascii_lowercase());
        }
        else {
            result.push('_');
        }
    }

    if result.is_empty() { "run_fixture".to_string() } else { result }
}

fn run_legion_dry_run(project_dir: &Path, target: RunFixtureTarget, output_dir: &Path) -> Result<bool, String> {
    use legion::cmds::run::{RunArgs, run as run_run};

    let args = RunArgs {
        project_dir: project_dir.to_path_buf(),
        target: target.canonical_target(),
        output_dir: Some(output_dir.to_path_buf()),
        workspace: false,
        runner: Vec::new(),
        artifact: None,
        dry_run: true,
        debug_artifacts: false,
        workload_json: None,
        workload_file: None,
    };

    run_run(&args).map(|code| code == ExitCode::SUCCESS).map_err(|error| error.to_string())
}

fn prepare_runtime_command(output_dir: &Path, target: RunFixtureTarget, run_contract: Option<&RunContract>) -> Result<PreparedCommand, String> {
    let family = runtime_family_for(target);
    let artifact = discover_artifact(output_dir, family, run_contract)?;
    let classpath = if artifact.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("jar")) {
        artifact.clone()
    }
    else {
        output_dir.to_path_buf()
    };
    let entry = run_contract
        .filter(|contract| !contract.logical_entry.is_empty())
        .map(|contract| contract.logical_entry.clone())
        .unwrap_or_else(|| artifact.file_stem().and_then(|value| value.to_str()).unwrap_or_default().to_string());
    let template = family.default_template(run_contract.map(|contract| RunnerRuntimeContract {
        logical_entry: (!contract.logical_entry.is_empty()).then_some(contract.logical_entry.as_str()),
        physical_entry: (!contract.physical_entry.is_empty()).then_some(contract.physical_entry.as_str()),
        wasi_p3: false,
    }));
    let command = template.prepare_command(&artifact, &classpath, &entry);

    Ok(PreparedCommand { program: command.command, args: command.args })
}

fn runtime_family_for(target: RunFixtureTarget) -> RuntimeFamily {
    match target {
        RunFixtureTarget::Clr => RuntimeFamily::Clr,
        RunFixtureTarget::Jvm => RuntimeFamily::Jvm,
        RunFixtureTarget::Node => RuntimeFamily::Node,
        RunFixtureTarget::Wasi => RuntimeFamily::Wasi,
    }
}

fn discover_artifact(output_dir: &Path, family: RuntimeFamily, run_contract: Option<&RunContract>) -> Result<PathBuf, String> {
    let mut files = fs::read_dir(output_dir)
        .map_err(|error| format!("failed to list '{}': {}", output_dir.display(), error))?
        .filter_map(|entry| entry.ok().map(|item| item.path()))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    files.sort();

    if let Some(contract) = run_contract {
        if let Some(path) = files.iter().find(|path| {
            path.file_name().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case(&contract.physical_entry))
        }) {
            return Ok(path.clone());
        }
    }

    let preferred = match family {
        RuntimeFamily::Clr => &["dll", "exe"][..],
        RuntimeFamily::Jvm => &["jar", "class"][..],
        RuntimeFamily::Node => &["mjs", "js", "wasm"][..],
        RuntimeFamily::Windows => &["exe"][..],
        RuntimeFamily::Wasi => &["wasm"][..],
        RuntimeFamily::Ovm => &["legion", "json"][..],
    };

    for extension in preferred {
        if let Some(path) = files
            .iter()
            .find(|path| path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case(extension)))
        {
            return Ok(path.clone());
        }
    }

    Err(format!("no runnable artifact found in '{}'", output_dir.display()))
}

fn run_runtime_command(
    project_dir: &Path,
    output_dir: &Path,
    target: RunFixtureTarget,
    prepared: &PreparedCommand,
) -> std::io::Result<std::process::Output> {
    let mut command = Command::new(&prepared.program);
    command.args(&prepared.args).current_dir(project_dir).stdin(std::process::Stdio::null());

    if target == RunFixtureTarget::Wasi {
        let _guard = WASI_RUNTIME_ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let config_path = output_dir.join("wasmtime-config.toml");
        let home_dir = output_dir.join("wasmtime-home");
        let local_app_data_dir = output_dir.join("wasmtime-local-app-data");
        let temp_dir = output_dir.join("wasmtime-tmp");
        fs::create_dir_all(&home_dir)?;
        fs::create_dir_all(&local_app_data_dir)?;
        fs::create_dir_all(&temp_dir)?;
        fs::write(&config_path, "[cache]\nenabled = false\n")?;
        command.env("WASMTIME_CONFIG_FILE", &config_path);
        command.env("WASMTIME_HOME", &home_dir);
        command.env("LOCALAPPDATA", &local_app_data_dir);
        command.env("APPDATA", &home_dir);
        command.env("TEMP", &temp_dir);
        command.env("TMP", &temp_dir);
        return command.output();
    }

    command.output()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreparedCommand {
    program: String,
    args: Vec<String>,
}

fn normalize_output_lines(content: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(content).replace("\r\n", "\n").lines().map(normalize_temp_dir_in_line).collect()
}

/// 将 wasmtime 报错信息中的随机临时目录路径规范化为 `<tmp>/legion-run-<stem>`，
/// 避免 `legion-run-{stem}{random}` 后缀导致 sidecar 比对不稳定。
fn normalize_temp_dir_in_line(line: &str) -> String {
    // 规范化 JVM/CLR 异常详情，`VerifyError`/`InvalidProgramException`/`NoSuchMethodError`
    // 的具体描述在不同运行中可能不同。
    let line = normalize_exception_details(line);
    // 规范化 JVM 本地化乱码行（包含 replacement character 的行）为 `<localized>`。
    let line = normalize_localized_jvm_line(&line);
    // 规范化临时目录路径中的随机后缀。
    normalize_legion_run_suffix(&line)
}

/// 规范化异常详情，保留异常类名，去除不稳定的描述文字。
fn normalize_exception_details(line: &str) -> String {
    const MARKERS: &[&str] = &["System.InvalidProgramException:", "java.lang.VerifyError:", "java.lang.NoSuchMethodError:"];
    for marker in MARKERS {
        if let Some(pos) = line.find(marker) {
            let prefix = &line[..pos + marker.len()];
            return format!("{prefix} <normalized>");
        }
    }
    line.to_string()
}

/// 将 JVM 本地化乱码行（包含 replacement character � 的行）替换为 `<localized>`。
fn normalize_localized_jvm_line(line: &str) -> String {
    if line.contains('\u{fffd}') {
        return "<localized>".to_string();
    }
    line.to_string()
}

/// 将 `legion-run-{name}{random}` 路径段中的 `{name}{random}` 替换为 `<normalized>`，
/// 避免 tempfile 随机后缀导致 sidecar 比对不稳定。
fn normalize_legion_run_suffix(line: &str) -> String {
    const MARKER: &str = "legion-run-";
    let Some(pos) = line.find(MARKER)
    else {
        return line.to_string();
    };
    let before = &line[..pos + MARKER.len()];
    let rest = &line[pos + MARKER.len()..];
    let stem_end = rest.find(|c: char| c == '\\' || c == '/').unwrap_or(rest.len());
    let after = &rest[stem_end..];
    format!("{before}<normalized>{after}")
}

fn collect_output_entries(output_dir: &Path) -> Vec<String> {
    let mut entries = Vec::new();
    collect_output_entries_recursive(output_dir, output_dir, &mut entries);
    entries.sort();
    entries
}

fn collect_output_entries_recursive(root: &Path, current: &Path, entries: &mut Vec<String>) {
    let Ok(read_dir) = fs::read_dir(current)
    else {
        return;
    };

    let mut children = read_dir.filter_map(|entry| entry.ok().map(|item| item.path())).collect::<Vec<_>>();
    children.sort();

    for child in children {
        let Ok(relative) = child.strip_prefix(root)
        else {
            continue;
        };
        let mut display = relative.to_string_lossy().replace('\\', "/");

        if child.is_dir() {
            display.push('/');
            entries.push(display);
            collect_output_entries_recursive(root, &child, entries);
            continue;
        }

        if child.is_file() {
            entries.push(display);
        }
    }
}

fn command_exists(command: &str) -> bool {
    let Some(path) = env::var_os("PATH")
    else {
        return false;
    };

    for dir in env::split_paths(&path) {
        for candidate in candidate_command_paths(&dir, command) {
            if candidate.is_file() {
                return true;
            }
        }
    }
    false
}

fn candidate_command_paths(dir: &Path, command: &str) -> Vec<PathBuf> {
    let base = dir.join(command);
    if Path::new(command).extension().is_some() {
        return vec![base];
    }

    let mut candidates = vec![base.clone()];
    if cfg!(windows) {
        let extensions = env::var("PATHEXT")
            .ok()
            .map(|value| value.split(';').filter(|item| !item.is_empty()).map(|item| item.to_ascii_lowercase()).collect::<Vec<_>>())
            .unwrap_or_else(|| vec![".exe".to_string(), ".cmd".to_string(), ".bat".to_string(), ".com".to_string()]);
        for ext in extensions {
            candidates.push(dir.join(format!("{command}{ext}")));
        }
    }
    candidates
}
