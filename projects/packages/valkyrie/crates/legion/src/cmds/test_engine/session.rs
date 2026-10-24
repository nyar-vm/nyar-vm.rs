//! 测试构建与执行会话。

use legion_workspace::{
    manifest::RunnerBinding,
    planner::{BuildRequest, WorkspaceResolver},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

use miette::{Result, miette};
use nyar_runner::{RuntimeContract as InterpreterRuntimeContract, RuntimeFamily as InterpreterRuntimeFamily};
use rusty_valkyrie::{CanonicalTarget, RunnerFamily};

use crate::cmds::{
    build::compile_plan,
    report::TestResultEntry,
    run::{ExecutionManifest, RunContract, select_artifact},
};

use super::{
    discover::{DiscoveredFunction, discover_project_tests},
    targets::parse_target_label,
};

/// 单次测试执行结果。
#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub success: bool,
    pub is_compile_error: bool,
    pub error: Option<String>,
}

/// 外部 target 测试会话：产物落盘后复用。
pub struct ExternalTestSession {
    pub target: String,
    pub output_dir: PathBuf,
    pub run_contracts: Vec<RunContract>,
}

/// 编译 target 测试会话。
pub fn compile_external_test_session(
    workspace: &WorkspaceResolver,
    project_dir: &Path,
    target_label: &str,
) -> Result<ExternalTestSession, String> {
    let target = parse_target_label(target_label).map_err(|e| e.to_string())?;
    let output_dir = project_dir.join(".cache").join("test").join(target_label);
    let request = BuildRequest { project_dir: project_dir.to_path_buf(), target, output_dir: Some(output_dir.clone()) };
    let (plan, _) = workspace.build_test_plan(&request).map_err(|e| e.to_string())?;
    compile_plan(&plan, false).map_err(|e| e.to_string())?;
    let manifest = ExecutionManifest::read_from_output_dir(&output_dir)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "compiler did not produce an execution manifest".to_string())?;
    if !manifest.is_fresh_for_plan(&plan).map_err(|e| e.to_string())? {
        return Err("compiler produced a stale execution manifest".into());
    }
    if manifest.run_contracts.is_empty() {
        return Err("compiler produced no runtime execution contracts".into());
    }
    Ok(ExternalTestSession { target: target_label.to_string(), output_dir, run_contracts: manifest.run_contracts })
}

/// 在外部会话中执行单个测试函数。
pub fn run_external_test_in_session(
    workspace: &WorkspaceResolver,
    session: &ExternalTestSession,
    function_name: &str,
    cli_runners: &[String],
    verbose: bool,
) -> RunOutcome {
    let Some(artifact) = resolve_external_test_artifact_path(session, function_name)
    else {
        return RunOutcome {
            success: false,
            is_compile_error: true,
            error: Some(format!("未找到测试 `{function_name}` 对应的产物。{}", describe_runnable_artifacts(session))),
        };
    };

    let Ok(canonical) = parse_target_label(&session.target)
    else {
        return RunOutcome { success: false, is_compile_error: true, error: Some(format!("未知目标 {}", session.target)) };
    };

    let runner_family = runner_family_for_label(&session.target);
    let template = match resolve_runner_template(workspace, &canonical, runner_family, cli_runners, function_name) {
        Ok(t) => t,
        Err(error) => return RunOutcome { success: false, is_compile_error: true, error: Some(error.to_string()) },
    };

    let placeholders = build_placeholders(&session.output_dir, &artifact, runner_family, function_name);
    let command = expand_placeholders(&template.command, &placeholders);
    let args: Vec<String> = template.args.iter().map(|a| expand_placeholders(a, &placeholders)).collect();

    let output = Command::new(&command).args(&args).stdout(Stdio::piped()).stderr(Stdio::piped()).output();

    match output {
        Ok(output) if output.status.success() => {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if !stderr.trim().is_empty() {
                return RunOutcome { success: false, is_compile_error: false, error: Some(stderr) };
            }
            if verbose {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if !stdout.trim().is_empty() {
                    println!("      stdout: {}", stdout.trim_end());
                }
            }
            RunOutcome { success: true, is_compile_error: false, error: None }
        }
        Ok(output) => {
            let mut error = String::from_utf8_lossy(&output.stderr).to_string();
            if error.trim().is_empty() {
                error = String::from_utf8_lossy(&output.stdout).to_string();
            }
            RunOutcome { success: false, is_compile_error: false, error: Some(error) }
        }
        Err(error) => RunOutcome { success: false, is_compile_error: false, error: Some(error.to_string()) },
    }
}

/// 为项目在多 target 上运行全部测试。
pub fn run_tests_for_project(
    workspace: &WorkspaceResolver,
    project_dir: &Path,
    filter: Option<&str>,
    targets: &[String],
    cli_runners: &[String],
    verbose: bool,
) -> (usize, usize, usize, Vec<TestResultEntry>) {
    let test_dir = project_dir.join("test");
    if !test_dir.exists() {
        println!("  无 test/ 目录，跳过");
        return (0, 0, 0, Vec::new());
    }

    let functions = discover_project_tests(project_dir);
    if functions.is_empty() {
        println!("  未发现 [test] 标注的测试函数");
        return (0, 0, 0, Vec::new());
    }

    let mut total_passed = 0usize;
    let mut total_failed = 0usize;
    let mut total_skipped = 0usize;
    let mut results = Vec::new();

    for target in targets {
        println!("  --- {target} ---");
        let session = compile_external_test_session(workspace, project_dir, target);

        for tf in &functions {
            if filter.is_some_and(|f| !tf.name.to_ascii_lowercase().contains(&f.to_ascii_lowercase())) {
                if verbose {
                    println!("    - {} ... 已跳过（过滤）", tf.name);
                }
                total_skipped += 1;
                results.push(TestResultEntry {
                    name: tf.name.clone(),
                    status: "skip".into(),
                    error: Some("被过滤器排除".into()),
                    target: target.clone(),
                });
                continue;
            }

            if tf.is_benchmark {
                if verbose {
                    println!("    - {} ... 已跳过（[benchmark]）", tf.name);
                }
                total_skipped += 1;
                results.push(TestResultEntry {
                    name: tf.name.clone(),
                    status: "skip".into(),
                    error: Some("[benchmark] 标注".into()),
                    target: target.clone(),
                });
                continue;
            }

            if tf.compile_only {
                if verbose {
                    println!("    - {} ... 已跳过（compile_only）", tf.name);
                }
                total_skipped += 1;
                results.push(TestResultEntry {
                    name: tf.name.clone(),
                    status: "skip".into(),
                    error: Some("compile_only 夹具（见 legion-language 编译测试）".into()),
                    target: target.clone(),
                });
                continue;
            }

            if !tf.is_test {
                continue;
            }

            print!("    {} ... ", tf.name);

            let outcome = match &session {
                Ok(session) => run_external_test_in_session(workspace, session, &tf.name, cli_runners, verbose),
                Err(error) => RunOutcome { success: false, is_compile_error: true, error: Some(error.clone()) },
            };

            if outcome.success {
                println!("ok");
                total_passed += 1;
                results.push(TestResultEntry { name: tf.name.clone(), status: "pass".into(), error: None, target: target.clone() });
            }
            else if outcome.is_compile_error {
                println!("COMPILE ERROR");
                if let Some(error) = &outcome.error {
                    println!("      {error}");
                }
                total_failed += 1;
                results.push(TestResultEntry {
                    name: tf.name.clone(),
                    status: "compile_error".into(),
                    error: outcome.error,
                    target: target.clone(),
                });
            }
            else {
                println!("FAILED");
                if let Some(error) = &outcome.error {
                    println!("      {error}");
                }
                total_failed += 1;
                results.push(TestResultEntry { name: tf.name.clone(), status: "fail".into(), error: outcome.error, target: target.clone() });
            }
        }
    }

    (total_passed, total_failed, total_skipped, results)
}

/// 为项目在多 target 上运行基准测试。
pub fn bench_project(
    workspace: &WorkspaceResolver,
    project_dir: &Path,
    project_name: &str,
    runs: usize,
    targets: &[String],
    verbose: bool,
) -> Vec<crate::cmds::report::BenchResultRow> {
    let functions: Vec<DiscoveredFunction> = discover_project_tests(project_dir).into_iter().filter(|f| f.is_benchmark).collect();
    if functions.is_empty() {
        println!("  未发现 [benchmark] 标注的函数");
        return Vec::new();
    }

    let mut results = Vec::new();
    for target in targets {
        for tf in &functions {
            let mut compile_times = Vec::new();
            let mut runtime_times = Vec::new();

            for _ in 0..runs {
                let compile_start = Instant::now();
                let session_result = compile_external_test_session(workspace, project_dir, target);
                let compile_ms = compile_start.elapsed().as_secs_f64() * 1000.0;

                let Ok(session) = session_result
                else {
                    continue;
                };

                let runtime_start = Instant::now();
                let outcome = run_external_test_in_session(workspace, &session, &tf.name, &[], verbose);
                let runtime_ms = runtime_start.elapsed().as_secs_f64() * 1000.0;

                if !outcome.success && verbose {
                    if let Some(error) = &outcome.error {
                        println!("      bench {} 失败: {error}", tf.name);
                    }
                }

                compile_times.push(compile_ms);
                runtime_times.push(runtime_ms);
            }

            if !compile_times.is_empty() && !runtime_times.is_empty() {
                let compile_avg = compile_times.iter().sum::<f64>() / compile_times.len() as f64;
                let runtime_avg = runtime_times.iter().sum::<f64>() / runtime_times.len() as f64;
                results.push(crate::cmds::report::BenchResultRow {
                    project: project_name.to_string(),
                    test: tf.name.clone(),
                    target: target.clone(),
                    compile_ms: compile_avg,
                    runtime_ms: runtime_avg,
                });
            }
        }
    }
    results
}

/// 只消费当前 Compiler 给出的精确测试运行合同。
pub fn resolve_external_test_artifact_path(session: &ExternalTestSession, function_name: &str) -> Option<PathBuf> {
    let mut contracts = session.run_contracts.iter().filter(|contract| contract.logical_entry == function_name);
    let contract = contracts.next()?;
    if contracts.next().is_some() {
        return None;
    }
    select_artifact(&session.output_dir, std::iter::once(contract.physical_entry.as_str()), None).ok().map(|(artifact, _)| artifact)
}

fn describe_runnable_artifacts(session: &ExternalTestSession) -> String {
    let names =
        session.run_contracts.iter().map(|contract| format!("{} → {}", contract.logical_entry, contract.physical_entry)).collect::<Vec<_>>();
    format!("Compiler 运行合同：{}", names.join(", "))
}

#[derive(Debug, Clone)]
struct RunnerTemplate {
    command: String,
    args: Vec<String>,
}

fn runner_family_for_label(label: &str) -> RunnerFamily {
    match label.to_ascii_lowercase().as_str() {
        "clr" => RunnerFamily::Clr,
        "jvm" => RunnerFamily::Jvm,
        "node" => RunnerFamily::Node,
        "legion" => RunnerFamily::Ovm,
        "wasi" => RunnerFamily::Wasi,
        _ => RunnerFamily::Ovm,
    }
}

fn resolve_runner_template(
    workspace: &WorkspaceResolver,
    canonical_target: &CanonicalTarget,
    runner_target: RunnerFamily,
    cli_runner_overrides: &[String],
    function_name: &str,
) -> Result<RunnerTemplate> {
    let default = default_runner_template(runner_target, function_name);

    if let Some(command) = parse_runner_overrides(cli_runner_overrides)?.remove(&runner_target) {
        return Ok(RunnerTemplate { command, args: default.args });
    }

    if let Some(workspace_manifest) = &workspace.workspace_manifest {
        if let Some(binding) = workspace_manifest.runner.iter().find(|binding| runner_binding_matches(binding, runner_target, canonical_target))
        {
            return Ok(RunnerTemplate { command: binding.command.clone(), args: binding.args.clone() });
        }
    }

    if let Ok(command) = std::env::var(format!("LEGION_RUNNER_{}", runner_target.as_str().to_ascii_uppercase())) {
        if !command.trim().is_empty() {
            return Ok(RunnerTemplate { command, args: default.args });
        }
    }

    Ok(default)
}

fn runner_binding_matches(binding: &RunnerBinding, runner_target: RunnerFamily, canonical_target: &CanonicalTarget) -> bool {
    binding.target.matches(runner_target, canonical_target)
}

fn parse_runner_overrides(values: &[String]) -> Result<BTreeMap<RunnerFamily, String>> {
    let mut overrides = BTreeMap::new();
    for item in values {
        let Some((target, command)) = item.split_once('=')
        else {
            return Err(miette!("invalid runner override '{item}': expected target=command"));
        };
        let family = target.trim().parse::<RunnerFamily>().map_err(|error| miette!("invalid runner override '{item}': {error}"))?;
        overrides.insert(family, command.trim().to_string());
    }
    Ok(overrides)
}

fn default_runner_template(target: RunnerFamily, function_name: &str) -> RunnerTemplate {
    let contract = InterpreterRuntimeContract { logical_entry: Some(function_name), physical_entry: Some(function_name), wasi_p3: false };
    let family = match target {
        RunnerFamily::Clr => InterpreterRuntimeFamily::Clr,
        RunnerFamily::Jvm => InterpreterRuntimeFamily::Jvm,
        RunnerFamily::Node => InterpreterRuntimeFamily::Node,
        RunnerFamily::Windows => InterpreterRuntimeFamily::Windows,
        RunnerFamily::Wasi => InterpreterRuntimeFamily::Wasi,
        RunnerFamily::Ovm => InterpreterRuntimeFamily::Ovm,
    };
    let template = family.default_template(Some(contract));
    RunnerTemplate { command: template.command, args: template.args }
}

fn build_placeholders(output_dir: &Path, artifact: &Path, runner_target: RunnerFamily, function_name: &str) -> BTreeMap<&'static str, String> {
    let mut values = BTreeMap::new();
    let artifact_text = strip_verbatim_prefix(&artifact.to_string_lossy()).to_string();
    values.insert("artifact", artifact_text.clone());
    let classpath = if artifact.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("jar")) {
        artifact_text.clone()
    }
    else {
        strip_verbatim_prefix(&output_dir.to_string_lossy()).to_string()
    };
    values.insert("classpath", classpath);
    let entry = if runner_target == RunnerFamily::Jvm { function_name.to_string() } else { function_name.to_string() };
    values.insert("entry", entry);
    values
}

fn strip_verbatim_prefix(path: &str) -> &str {
    path.strip_prefix(r"\\?\").unwrap_or(path)
}

fn expand_placeholders(template: &str, placeholders: &BTreeMap<&'static str, String>) -> String {
    placeholders.iter().fold(template.to_string(), |current, (key, value)| current.replace(&format!("{{{key}}}"), value))
}
