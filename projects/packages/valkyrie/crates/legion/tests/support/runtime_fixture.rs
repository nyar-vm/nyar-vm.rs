use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use legion::{
    CanonicalTarget,
    cmds::{
        build::{BuildArgs, run as run_build},
        run::RunContract as LegionRunContract,
    },
};
use nyar::{
    RuntimeFixtureResult, collect_fixture_cases_with_extensions, load_runtime_fixture_spec, resolve_runtime_fixture_targets,
    verify_runtime_fixture_case,
};
use nyar_runner::{RuntimeContract, RuntimeFamily};

use crate::support::{SmokeProject, create_smoke_project_with_manifest};

const DEFAULT_RUNTIME_FIXTURE_TARGETS: [RuntimeFixtureTarget; 5] = [
    RuntimeFixtureTarget::Clr,
    RuntimeFixtureTarget::Jvm,
    RuntimeFixtureTarget::Node,
    RuntimeFixtureTarget::Wasi,
    RuntimeFixtureTarget::Native,
];
const DEFAULT_RUNTIME_FIXTURE_TARGET_NAMES: [&str; 5] = ["clr", "jvm", "node", "wasi", "native"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuntimeFixtureTarget {
    Clr,
    Jvm,
    Node,
    Wasi,
    Native,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedRunContract {
    logical_entry: String,
    physical_entry: String,
}

impl RuntimeFixtureTarget {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "clr" => Some(Self::Clr),
            "jvm" => Some(Self::Jvm),
            "node" => Some(Self::Node),
            "wasi" => Some(Self::Wasi),
            "native" => Some(Self::Native),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clr => "clr",
            Self::Jvm => "jvm",
            Self::Node => "node",
            Self::Wasi => "wasi",
            Self::Native => "native",
        }
    }

    fn canonical_target(self) -> CanonicalTarget {
        match self {
            Self::Clr => CanonicalTarget::clr(),
            Self::Jvm => CanonicalTarget::parse("jvm-openjdk-unknown-managed").unwrap(),
            Self::Node => CanonicalTarget::parse("node").unwrap(),
            Self::Wasi => CanonicalTarget::parse("wasi").unwrap(),
            Self::Native => {
                #[cfg(windows)]
                {
                    CanonicalTarget::parse("x86_64-pc-windows-msvc").unwrap()
                }
                #[cfg(all(unix, not(target_os = "macos")))]
                {
                    CanonicalTarget::parse("x86_64-unknown-linux-gnu").unwrap()
                }
                #[cfg(target_os = "macos")]
                {
                    CanonicalTarget::parse("x86_64-apple-darwin").unwrap()
                }
            }
        }
    }

    fn manifest_target(self) -> &'static str {
        match self {
            Self::Clr => "clr",
            Self::Jvm => "jvm-openjdk-unknown-managed",
            Self::Node => "node",
            Self::Wasi => "wasi",
            Self::Native => {
                #[cfg(windows)]
                {
                    "x86_64-pc-windows-msvc"
                }
                #[cfg(all(unix, not(target_os = "macos")))]
                {
                    "x86_64-unknown-linux-gnu"
                }
                #[cfg(target_os = "macos")]
                {
                    "x86_64-apple-darwin"
                }
            }
        }
    }

    fn runtime_family(self) -> RuntimeFamily {
        match self {
            Self::Clr => RuntimeFamily::Clr,
            Self::Jvm => RuntimeFamily::Jvm,
            Self::Node => RuntimeFamily::Node,
            Self::Wasi => RuntimeFamily::Wasi,
            Self::Native => RuntimeFamily::Windows,
        }
    }

    fn required_command(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Clr => Some(("dotnet", "dotnet")),
            Self::Jvm => Some(("java", "java")),
            Self::Node => Some(("node", "node")),
            Self::Wasi => Some(("wasmtime", "wasmtime")),
            Self::Native => None,
        }
    }
}

pub fn regenerate_enabled() -> bool {
    ["VALKYRIE_TEST_REGENERATE", "LEGION_TEST_REGENERATE", "NYAR_TEST_REGENERATE"].into_iter().any(env_flag_enabled)
}

fn env_flag_enabled(name: &str) -> bool {
    env::var(name).ok().is_some_and(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on" | "regenerate"))
}

pub fn collect_runtime_fixture_cases(root: &Path) -> Vec<PathBuf> {
    let mut cases = collect_fixture_cases_with_extensions(root, &["valkyrie"]);
    // 支持通过环境变量 `RUNTIME_FIXTURE_FILTER` 按子串过滤 fixture 名，便于聚焦调试。
    // 多个过滤词以分号分隔；任一匹配即保留。空值不过滤。
    if let Ok(filter_str) = env::var("RUNTIME_FIXTURE_FILTER") {
        let filters: Vec<&str> = filter_str.split(';').map(str::trim).filter(|s| !s.is_empty()).collect();
        if !filters.is_empty() {
            cases.retain(|path| {
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                filters.iter().any(|filter| name.contains(filter))
            });
        }
    }
    cases
}

pub fn can_run_all_required_commands(fixtures: &[PathBuf]) -> bool {
    let mut required = BTreeSet::new();
    for fixture_path in fixtures {
        for target in fixture_targets_for_path(fixture_path) {
            if let Some((label, command)) = target.required_command() {
                required.insert((label, command));
            }
        }
    }

    for (label, command) in required {
        if !command_exists(command) {
            eprintln!("skip runtime fixture: missing {}", label);
            return false;
        }
    }
    true
}

pub fn verify_runtime_fixture(fixture_path: &Path) {
    let source = fs::read_to_string(fixture_path)
        .unwrap_or_else(|error| panic!("failed to read runtime fixture '{}': {}", fixture_path.display(), error));
    let project_name = runtime_project_name(fixture_path);
    let targets = fixture_targets_for_path(fixture_path);
    let project = create_runtime_smoke_project(fixture_path, &project_name, &source, &targets);
    let existing_spec = load_runtime_fixture_spec(fixture_path);

    verify_runtime_fixture_case(fixture_path, &DEFAULT_RUNTIME_FIXTURE_TARGET_NAMES, regenerate_enabled(), |target| {
        let target = RuntimeFixtureTarget::parse(target)
            .unwrap_or_else(|| panic!("runtime fixture '{}' contains unsupported target '{}'", fixture_path.display(), target));
        let expected_for_target = existing_spec.as_ref().and_then(|spec| spec.expect.get(target.as_str()));
        let allow_stderr = expected_for_target.is_some_and(|result| result.allow_stderr || !result.stderr.is_empty());
        let record_allow_stderr = expected_for_target.is_some_and(|result| result.allow_stderr);
        let mut result = execute_runtime_target(&project, &project_name, target, allow_stderr, record_allow_stderr);
        // fixture 可声明"预期非零退出码"语义：当 expected.success=true 且 expected.result
        // 与实际退出码精确匹配时，OS 层的非零退出码不再视为失败。这覆盖 catch 等
        // 以 ExitCode(value) 表达程序语义的场景。
        if let Some(expected) = expected_for_target {
            if expected.success && expected.result == result.result && !result.success {
                result.success = true;
                result.errors.clear();
            }
        }
        result
    });
}

fn fixture_targets_for_path(fixture_path: &Path) -> Vec<RuntimeFixtureTarget> {
    resolve_runtime_fixture_targets(fixture_path, &DEFAULT_RUNTIME_FIXTURE_TARGET_NAMES)
        .into_iter()
        .map(|target| {
            RuntimeFixtureTarget::parse(&target)
                .unwrap_or_else(|| panic!("runtime fixture '{}' contains unsupported target '{}'", fixture_path.display(), target))
        })
        .collect()
}

fn runtime_project_name(fixture_path: &Path) -> String {
    let stem = fixture_path.file_stem().and_then(|value| value.to_str()).unwrap_or("runtime_fixture");
    format!("runtime_{}", sanitize_identifier(stem))
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

    if result.is_empty() { "runtime_fixture".to_string() } else { result }
}

fn create_runtime_smoke_project(fixture_path: &Path, project_name: &str, source: &str, targets: &[RuntimeFixtureTarget]) -> SmokeProject {
    let prefix =
        format!("legion-runtime-{}", sanitize_identifier(fixture_path.file_stem().and_then(|value| value.to_str()).unwrap_or("fixture")));
    let manifest = runtime_project_manifest(project_name, targets);
    create_smoke_project_with_manifest(&prefix, &manifest, source)
}

fn runtime_project_manifest(project_name: &str, targets: &[RuntimeFixtureTarget]) -> String {
    let build = targets
        .iter()
        .map(|target| match target {
            RuntimeFixtureTarget::Clr => r#"        {
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

fn execute_runtime_target(
    project: &SmokeProject,
    project_name: &str,
    target: RuntimeFixtureTarget,
    allow_stderr: bool,
    record_allow_stderr: bool,
) -> RuntimeFixtureResult {
    let output_dir = project.project_dir.join("dist").join(target.as_str());
    let build_status = run_build(&BuildArgs {
        project_dir: project.project_dir.clone(),
        target: target.canonical_target(),
        output_dir: Some(output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    });

    let Ok(build_status) = build_status
    else {
        let error = build_status.err().unwrap();
        return failed_result(format!("build failed for {}: {}", target.as_str(), error));
    };

    if build_status != ExitCode::SUCCESS {
        return failed_result(format!("build returned non-zero status for {}", target.as_str()));
    }

    let command = match prepare_runtime_command(&output_dir, project_name, target.runtime_family()) {
        Ok(command) => command,
        Err(error) => return failed_result(format!("failed to prepare runtime command for {}: {}", target.as_str(), error)),
    };
    capture_runtime_command(&project.project_dir, &command.command, &command.args, allow_stderr, record_allow_stderr)
}

fn prepare_runtime_command(output_dir: &Path, project_name: &str, family: RuntimeFamily) -> Result<PreparedCommand, String> {
    let contracts = read_run_contracts(output_dir)?;
    let (artifact, contract) = discover_artifact(output_dir, project_name, family, &contracts)?;
    let classpath = if artifact.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("jar")) {
        artifact.clone()
    }
    else {
        output_dir.to_path_buf()
    };
    let entry = if contract.logical_entry.is_empty() {
        artifact.file_stem().and_then(|value| value.to_str()).unwrap_or_default().to_string()
    }
    else {
        contract.logical_entry.clone()
    };
    let template = family.default_template(Some(RuntimeContract {
        logical_entry: (!contract.logical_entry.is_empty()).then_some(contract.logical_entry.as_str()),
        physical_entry: Some(contract.physical_entry.as_str()),
        wasi_p3: false,
    }));
    let command = template.prepare_command(&artifact, &classpath, &entry);
    Ok(PreparedCommand { command: command.command, args: command.args })
}

fn read_run_contracts(output_dir: &Path) -> Result<Vec<ParsedRunContract>, String> {
    let contracts = LegionRunContract::read_all_from_output_dir(output_dir)
        .map_err(|error| format!("failed to read run-contracts.txt in '{}': {}", output_dir.display(), error))?;
    if contracts.is_empty() {
        return Err(format!("missing run-contracts.txt in '{}'", output_dir.display()));
    }
    Ok(contracts
        .into_iter()
        .map(|contract| ParsedRunContract { logical_entry: contract.logical_entry, physical_entry: contract.physical_entry })
        .collect())
}

fn discover_artifact(
    output_dir: &Path,
    project_name: &str,
    family: RuntimeFamily,
    contracts: &[ParsedRunContract],
) -> Result<(PathBuf, ParsedRunContract), String> {
    let mut files = fs::read_dir(output_dir)
        .map_err(|error| format!("failed to list '{}': {}", output_dir.display(), error))?
        .filter_map(|entry| entry.ok().map(|item| item.path()))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    files.sort();

    for contract in contracts {
        if contract.physical_entry.is_empty() {
            continue;
        }
        if let Some(path) = files.iter().find(|path| {
            path.file_name().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case(&contract.physical_entry))
        }) {
            return Ok((path.clone(), contract.clone()));
        }
    }

    let contract = contracts.first().cloned().unwrap_or(ParsedRunContract { logical_entry: String::new(), physical_entry: String::new() });

    let preferred = match family {
        RuntimeFamily::Clr => &["dll", "exe"][..],
        RuntimeFamily::Jvm => &["jar", "class"][..],
        RuntimeFamily::Node => &["mjs", "js", "wasm"][..],
        RuntimeFamily::Windows => &["exe", ""][..],
        RuntimeFamily::Wasi => &["wasi", "wasm"][..],
        RuntimeFamily::Ovm => &["legion", "json"][..],
    };

    for extension in preferred {
        if let Some(path) = files.iter().find(|path| {
            path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case(extension))
                && path.file_stem().and_then(|value| value.to_str()).is_some_and(|value| matches_project_name(value, project_name))
        }) {
            return Ok((path.clone(), contract));
        }
    }

    for extension in preferred {
        if let Some(path) = files
            .iter()
            .find(|path| path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case(extension)))
        {
            return Ok((path.clone(), contract));
        }
    }

    Err(format!("no runnable artifact found in '{}'", output_dir.display()))
}

fn matches_project_name(file_stem: &str, project_name: &str) -> bool {
    file_stem.eq_ignore_ascii_case(project_name)
        || file_stem.replace('_', ".").eq_ignore_ascii_case(project_name)
        || file_stem.replace('.', "_").eq_ignore_ascii_case(project_name)
}

fn capture_runtime_command(cwd: &Path, program: &str, args: &[String], allow_stderr: bool, record_allow_stderr: bool) -> RuntimeFixtureResult {
    match Command::new(program).args(args).current_dir(cwd).output() {
        Ok(output) => {
            let success = process_success_with_stderr_policy(output.status.success(), &output.stderr, allow_stderr);
            let stdout = normalize_output_lines(&output.stdout);
            let stderr = normalize_output_lines(&output.stderr);
            let result = output.status.code();
            let errors = if success { Vec::new() } else { vec![format!("process exited with code {}", result.unwrap_or(1))] };
            RuntimeFixtureResult { success, stdout, stderr, allow_stderr: record_allow_stderr, errors, result }
        }
        Err(error) => failed_result(format!("failed to start '{}': {}", program, error)),
    }
}

fn normalize_output_lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n").lines().map(|line| line.to_string()).collect()
}

fn process_success_with_stderr_policy(exit_success: bool, stderr: &[u8], allow_stderr: bool) -> bool {
    if !exit_success {
        return false;
    }
    if allow_stderr {
        return true;
    }
    String::from_utf8_lossy(stderr).trim().is_empty()
}

fn failed_result(message: String) -> RuntimeFixtureResult {
    RuntimeFixtureResult { success: false, stdout: Vec::new(), stderr: Vec::new(), allow_stderr: false, errors: vec![message], result: None }
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

struct PreparedCommand {
    command: String,
    args: Vec<String>,
}
