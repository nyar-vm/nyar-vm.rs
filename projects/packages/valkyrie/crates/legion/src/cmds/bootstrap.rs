#![doc = include_str!("readme.md")]

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use clap::Args;
use miette::{IntoDiagnostic, Report, Result, miette};
use rusty_valkyrie::CanonicalTarget;

use crate::cmds::run::{ExecutionManifest, select_artifact};

/// `legion bootstrap` 的命令参数。
#[derive(Debug, Clone, Args)]
pub struct BootstrapArgs {
    /// 项目目录，默认当前目录。
    #[arg(value_name = "project-dir", default_value = ".")]
    pub project_dir: PathBuf,
    /// 自举目标项目目录；若指定则覆盖 `project-dir`。
    /// 统一验收默认指向 `valkyrie.v/projects/legion._/projects/legion.tools`。
    #[arg(long = "project", value_name = "PATH")]
    pub bootstrap_project: Option<PathBuf>,
    /// seed 路径，用于自举的已有可运行二进制。
    #[arg(long = "seed")]
    pub seed_path: Option<PathBuf>,
    /// 是否跳过 v1/v2 比对。
    #[arg(long)]
    pub skip_compare: bool,
    /// 目标平台，默认 `clr`。
    #[arg(long, default_value = "clr")]
    pub target: CanonicalTarget,
}

/// 自举阶段枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapStage {
    /// 获取 seed。
    Seed,
    /// 编译 v1（seed 编译编译器源码）。
    V1,
    /// 运行 v1 验证。
    V1Run,
    /// 编译 v2（v1 编译同一份编译器源码）。
    V2,
    /// 比对 v1 和 v2。
    Compare,
}

impl std::fmt::Display for BootstrapStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Seed => write!(f, "seed"),
            Self::V1 => write!(f, "v1"),
            Self::V1Run => write!(f, "v1 运行验证"),
            Self::V2 => write!(f, "v2"),
            Self::Compare => write!(f, "v1/v2 比对"),
        }
    }
}

/// 自举结果。
#[derive(Debug)]
pub struct BootstrapResult {
    /// 成功的阶段。
    pub stages_completed: Vec<BootstrapStage>,
    /// 失败的阶段（如果有）。
    pub failed_stage: Option<(BootstrapStage, Report)>,
    /// v1 产物路径。
    pub v1_path: Option<PathBuf>,
    /// v2 产物路径。
    pub v2_path: Option<PathBuf>,
}

impl BootstrapResult {
    /// 检查是否全部成功。
    pub fn is_success(&self) -> bool {
        self.failed_stage.is_none()
    }

    /// 获取错误信息。
    pub fn error(&self) -> Option<&Report> {
        self.failed_stage.as_ref().map(|(_, report)| report)
    }
}

/// 执行自举流程。
pub fn run(args: &BootstrapArgs) -> Result<BootstrapResult> {
    #[cfg(not(feature = "bootstrap-lane"))]
    {
        return Err(miette!(
            "legion bootstrap 不属于 LeetCode 正式编译链；请使用 `legion build` / `legion run`（或显式启用 `bootstrap-lane` feature 构建）"
        ));
    }
    let project_dir = args.bootstrap_project.clone().unwrap_or_else(|| args.project_dir.clone());
    let mut result = BootstrapResult { stages_completed: Vec::new(), failed_stage: None, v1_path: None, v2_path: None };
    let target_str = args.target.to_string();
    // `WASI` 目标的规范字符串形如 `wasm32-unknown-wasi-wasi`，以 `wasm` 开头但包含 `wasi`。
    // 因此用 `contains("wasi")` 检测 `WASI` 目标，`contains("wasm")` 检测所有 `WASM` 系目标。
    let is_wasm_target = target_str.contains("wasm") || target_str.contains("wasi");
    let is_jvm_target = is_jvm_target(&target_str);

    // 阶段 1: 获取 seed。
    let seed_path = match resolve_seed(&args.seed_path, &project_dir) {
        Ok(path) => {
            result.stages_completed.push(BootstrapStage::Seed);
            println!("seed: {}", path.display());
            path
        }
        Err(e) => {
            result.failed_stage = Some((BootstrapStage::Seed, e));
            return Ok(result);
        }
    };

    // 阶段 2: 用 seed 编译编译器源码，得到 v1。
    let v1_output_dir = project_dir.join("dist").join("v1");
    let source_file = get_compiler_source_path(&project_dir);
    match compile_with_seed(&seed_path, &project_dir, &v1_output_dir, "v1", &target_str, &source_file) {
        Ok(_) => {
            result.stages_completed.push(BootstrapStage::V1);
            println!("v1: compiled");
        }
        Err(e) => {
            result.failed_stage = Some((BootstrapStage::V1, e));
            return Ok(result);
        }
    }

    // 阶段 3: 运行 v1 验证。
    let v1_artifact = match resolve_manifest_artifact(&v1_output_dir) {
        Ok(path) => path,
        Err(e) => {
            result.failed_stage = Some((BootstrapStage::V1, e));
            return Ok(result);
        }
    };
    // 对于 Wasm 家族目标，运行 v1 会产生一个输出文件（Node=`.wasm`，WASI=`.wasi`），用于后续比对。
    let v1_output_path =
        if is_wasm_target { Some(v1_output_dir.join(format!("v1_output.{}", wasm_family_extension(&target_str)))) } else { None };

    match run_and_verify(&v1_artifact, &target_str, v1_output_path.as_deref(), &source_file) {
        Ok(()) => {
            result.stages_completed.push(BootstrapStage::V1Run);
            println!("v1: verified");
        }
        Err(e) => {
            result.failed_stage = Some((BootstrapStage::V1Run, e));
            return Ok(result);
        }
    }

    // 阶段 4: 用 v1 编译同一份编译器源码，得到 v2。
    let v2_output_dir = project_dir.join("dist").join("v2");
    match compile_with_seed(&v1_artifact, &project_dir, &v2_output_dir, "v2", &target_str, &source_file) {
        Ok(()) => {
            result.stages_completed.push(BootstrapStage::V2);
            println!("v2: compiled");
        }
        Err(e) => {
            result.failed_stage = Some((BootstrapStage::V2, e));
            return Ok(result);
        }
    }

    let v2_artifact = match resolve_manifest_artifact(&v2_output_dir) {
        Ok(path) => path,
        Err(e) => {
            result.failed_stage = Some((BootstrapStage::V2, e));
            return Ok(result);
        }
    };
    // 阶段 5: 比对 v1 和 v2。
    if !args.skip_compare {
        let compare_result = if target_str.contains("wasi") {
            compare_artifacts(&v1_artifact, &v2_artifact)
        }
        else if is_wasm_target {
            let v2_output = v2_output_dir.join(format!("v2_output.{}", wasm_family_extension(&target_str)));
            compare_artifacts(&v1_output_path.unwrap(), &v2_output)
        }
        else if is_jvm_target {
            compare_jvm_contracts(&v1_output_dir, &v2_output_dir)
        }
        else {
            compare_clr_contracts(&v1_output_dir, &v2_output_dir)
        };

        match compare_result {
            Ok(()) => {
                result.stages_completed.push(BootstrapStage::Compare);
                println!("v1/v2 比对: 一致");
            }
            Err(e) => {
                result.failed_stage = Some((BootstrapStage::Compare, e));
                return Ok(result);
            }
        }
    }
    else {
        println!("v1/v2 比对: skipped");
    }

    result.v1_path = Some(v1_artifact);
    result.v2_path = Some(v2_artifact);
    Ok(result)
}

/// 解析 seed 路径。
///
/// 优先级：
/// 1. 用户指定的 --seed 参数
/// 2. 使用当前二进制作为 seed（self-seed）
///
/// 注意：seed 始终是可运行的 `legion` 二进制；后续阶段必须由产物自身完成编译。
fn resolve_seed(seed_path: &Option<PathBuf>, _project_dir: &Path) -> Result<PathBuf> {
    if let Some(path) = seed_path {
        if !path.exists() {
            return Err(miette!("seed 文件不存在: {}", path.display()));
        }
        return Ok(strip_verbatim_prefix(path));
    }

    // 使用当前二进制 self-seed。
    let current_exe = std::env::current_exe().into_diagnostic().map_err(|error| error.wrap_err("获取当前可执行文件路径失败"))?;
    let current_exe = fs::canonicalize(current_exe).into_diagnostic().map_err(|error| error.wrap_err("解析 seed 路径失败"))?;

    // 移除 `\\?\` 前缀，否则 `Command::new` 在 Windows 上会 panic。
    Ok(strip_verbatim_prefix(&current_exe))
}

/// 查找本机 `wasmtime` CLI。
///
/// `WASI` 目标不生成 `.mjs` 启动壳，而是直接通过本机 `wasmtime` 运行 `WASM` 模块。
fn find_wasmtime_cli() -> Result<PathBuf> {
    let path = env::var_os("PATH").ok_or_else(|| miette!("未设置 PATH，无法查找本机 `wasmtime`"))?;
    let extensions = executable_extensions();
    for dir in env::split_paths(&path) {
        for candidate in candidate_command_paths(&dir, "wasmtime", &extensions) {
            if candidate.is_file() {
                return Ok(strip_verbatim_prefix(&candidate));
            }
        }
    }

    Err(miette!("未在 PATH 中找到本机 `wasmtime`，请先安装并确保可直接执行"))
}

/// 移除 Windows extended-length path 前缀 `\\?\`。
fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    if let Some(stripped) = path_str.strip_prefix(r"\\?\") { PathBuf::from(stripped) } else { path.to_path_buf() }
}

/// 用 `.wasi` seed 经 wasmtime 执行 `build`，产出下一阶段编译器产物。
///
/// 对齐 `bootstrap-wasi.mjs` / `compileV2`：
/// `wasmtime run -W gc [-S p3] --dir … <seed>.wasi -- build <project> --target <target> -o <out>`
fn compile_wasi_seed_with_wasmtime(seed: &Path, project_dir: &Path, output_dir: &Path, stage_name: &str, target: &str) -> Result<()> {
    let wasmtime = find_wasmtime_cli()?;
    let stderr_path = output_dir.join(format!("{stage_name}_stderr.log"));
    let stderr_file = std::fs::File::create(&stderr_path)
        .into_diagnostic()
        .map_err(|error| error.wrap_err(format!("创建 stderr 日志文件失败 {}", stderr_path.display())))?;

    // Guest 需读写工程与依赖；挂载项目、输出，以及可见的 workspace 根（若可推断）。
    let mut preopens = vec![project_dir.to_path_buf(), output_dir.to_path_buf()];
    // Walk up to workspace root (`legions.von`), covering nested package paths:
    // `…/valkyrie.v/projects/legion._/projects/legion.tools` → `…/valkyrie.v`
    {
        let mut cursor = project_dir.parent();
        while let Some(dir) = cursor {
            preopens.push(dir.to_path_buf());
            if dir.join("legions.von").is_file() {
                if let Some(repo) = dir.parent() {
                    preopens.push(repo.to_path_buf());
                }
                break;
            }
            cursor = dir.parent();
        }
    }
    preopens.sort();
    preopens.dedup();

    let mut cmd = Command::new(&wasmtime);
    cmd.arg("run");
    cmd.arg("-W");
    cmd.arg("gc");
    cmd.arg("-W");
    cmd.arg("max-memory-size=16777216");
    if target.contains("wasip3") {
        cmd.arg("-S");
        cmd.arg("p3");
    }
    for dir in &preopens {
        cmd.arg("--dir");
        cmd.arg(dir);
    }
    cmd.arg(seed);
    cmd.arg("--");
    cmd.arg("build");
    cmd.arg(project_dir);
    cmd.arg("--target");
    cmd.arg(target);
    cmd.arg("-o");
    cmd.arg(output_dir);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::from(stderr_file));

    println!("{}: executing {:?}", stage_name, cmd);

    let output = cmd.output().into_diagnostic().map_err(|error| error.wrap_err(format!("{stage_name} wasmtime 运行失败")))?;

    if !output.status.success() {
        let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(miette!(
            r#"{stage_name} WASI seed 再编译失败
stdout: {stdout}
stderr: {stderr}"#,
            stage_name = stage_name,
            stdout = stdout.trim(),
            stderr = stderr.trim(),
        ));
    }

    match resolve_manifest_artifact(output_dir) {
        Ok(path) => {
            println!("  {} artifact: {}", stage_name, path.display());
            Ok(())
        }
        Err(_) => {
            let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
            let stdout = String::from_utf8_lossy(&output.stdout);
            Err(miette!(
                r#"{stage_name} WASI seed 退出码为 0 但未产出 `.wasi`
seed（{seed}）可能尚不具备 `legion build` 能力，或 FS/argv cabi 未打通。
stdout: {stdout}
stderr: {stderr}"#,
                stage_name = stage_name,
                seed = seed.display(),
                stdout = stdout.trim(),
                stderr = stderr.trim(),
            ))
        }
    }
}

/// 使用指定的 seed 编译项目。
///
/// 对于原生 seed（`legion.exe`）：直接运行 `seed build <project_dir> --target <target> --output <output_dir>`。
/// 对于 Wasm 家族 seed（`.wasm` / `.wasi` 文件）：
///   - `wasm` 目标：通过 `node <seed>.mjs` 运行（**非 CLR 发布门**，`micro_compiler` 已废弃）
///   - `wasi` / `wasip3` 目标：通过本机 `wasmtime run … <seed>.wasi -- build …` 再编译
fn compile_with_seed(seed: &Path, project_dir: &Path, output_dir: &Path, stage_name: &str, target: &str, source_file: &Path) -> Result<()> {
    fs::create_dir_all(output_dir).into_diagnostic().map_err(|error| error.wrap_err(format!("创建输出目录失败 {}", output_dir.display())))?;

    let normalized_seed = strip_verbatim_prefix(seed);
    let normalized_project_dir = strip_verbatim_prefix(project_dir);
    let normalized_output_dir = strip_verbatim_prefix(output_dir);

    // 检测 seed 是否为 Wasm 家族文件（v1 产物）。
    let is_wasm_seed = normalized_seed.extension().is_some_and(|ext| ext == "wasm" || ext == "wasi");
    // `WASI` 目标字符串形如 `wasm32-unknown-wasi-wasi`，用 `contains("wasi")` 检测。
    let is_wasi_target = target.contains("wasi");

    if is_wasm_seed {
        if !source_file.exists() {
            return Err(miette!(
                r#"{stage_name} 源文件不存在: {source_file}
诚实自举要求 v1 实际读取源文件内容"#,
                stage_name = stage_name,
                source_file = source_file.display()
            ));
        }

        if is_wasi_target {
            return compile_wasi_seed_with_wasmtime(&normalized_seed, &normalized_project_dir, &normalized_output_dir, stage_name, target);
        }

        // 对于 V2 阶段，输出文件名为 v2_output.<ext>；对于 V1 阶段，由调用方处理。
        let output_file = if stage_name == "v2" {
            normalized_output_dir.join(format!("v2_output.{}", wasm_family_extension(target)))
        }
        else {
            normalized_output_dir.join(format!("v1_output.{}", wasm_family_extension(target)))
        };
        // WASM 目标：通过 node 运行 .mjs 启动壳。
        let launcher_path = normalized_seed.with_extension("mjs");
        if !launcher_path.exists() {
            return Err(miette!(
                "{stage_name} WASM seed 缺少启动壳: {launcher_path}\n\
                 请确认 seed 编译时已生成 .mjs 启动壳",
                stage_name = stage_name,
                launcher_path = launcher_path.display()
            ));
        }
        let mut cmd = Command::new("node");
        cmd.arg(&launcher_path).arg(source_file).arg(&output_file);

        println!("{}: executing {:?}", stage_name, cmd);

        let output = cmd.output().into_diagnostic().map_err(|error| error.wrap_err(format!("{stage_name} WASM 运行失败")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(miette!(
                r#"{} WASM 运行失败
stdout: {}
stderr: {}"#,
                stage_name,
                stdout.trim(),
                stderr.trim()
            ));
        }

        // 验证产物确实被生成了。
        if !output_file.exists() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(miette!(
                r#"{stage_name} WASM 运行退出码为 0 但未产出产物 {path}
seed（{seed}）可能未正确实现输出逻辑。
stdout: {stdout}
stderr: {stderr}"#,
                stage_name = stage_name,
                path = output_file.display(),
                seed = seed.display(),
                stdout = stdout.trim(),
                stderr = stderr.trim(),
            ));
        }

        println!("  {} 输出: {} ({} 字节)", stage_name, output_file.display(), fs::metadata(&output_file).map(|m| m.len()).unwrap_or(0));

        return Ok(());
    }

    // 原生 seed 或自举产物：运行 build 命令。

    // 将 stderr 重定向到文件，避免 `Command::output()` 在 Windows 上
    // 因子进程写入 stderr 管道而 panic（Windows pipe 读取的已知问题）。
    let stderr_path = output_dir.join(format!("{stage_name}_stderr.log"));
    let stderr_file = std::fs::File::create(&stderr_path)
        .into_diagnostic()
        .map_err(|error| error.wrap_err(format!("创建 stderr 日志文件失败 {}", stderr_path.display())))?;

    let mut cmd = compiler_command(&normalized_seed);
    cmd.arg("build").arg(&normalized_project_dir).arg("--target").arg(target).arg("--output").arg(&normalized_output_dir);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::from(stderr_file));

    println!("{}: executing {:?}", stage_name, cmd);

    let output = cmd.output().into_diagnostic().map_err(|error| error.wrap_err(format!("{stage_name} 编译失败")))?;

    if !output.status.success() {
        let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(miette!(
            r#"{} 编译失败
stdout: {}
stderr: {}"#,
            stage_name,
            stdout.trim(),
            stderr.trim()
        ));
    }

    // 验证产物确实被生成了。
    match resolve_manifest_artifact(output_dir) {
        Ok(path) => {
            println!("  {} artifact: {}", stage_name, path.display());
            Ok(())
        }
        Err(_) => {
            let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
            let stdout = String::from_utf8_lossy(&output.stdout);
            Err(miette!(
                r#"{stage_name} 编译退出码为 0 但未产出 execution manifest 声明的产物
seed（{seed}）可能不具备 `legion build` 命令行能力。
stdout: {stdout}
stderr: {stderr}"#,
                stage_name = stage_name,
                seed = seed.display(),
                stdout = stdout.trim(),
                stderr = stderr.trim(),
            ))
        }
    }
}

/// 运行并验证产物。
///
/// 对于 `CLR` 目标，直接执行 `.exe` 并检查退出码。
/// 对于 `JVM` 目标，通过本机 `java -jar` 校验 `--version` / `--help`。
/// 对于 `WASM` 目标，通过 `node <artifact>.mjs <source_file> <output>` 运行，
///   微编译器读取源文件内容，产生输出文件用于后续比对。
/// 对于 `WASI` 目标，通过本机 `wasmtime run -W gc -W wmemcheck -W max-memory-size=...` 运行。
fn run_and_verify(artifact_path: &Path, target: &str, output_path: Option<&Path>, source_file: &Path) -> Result<()> {
    if !artifact_path.exists() {
        return Err(miette!("产物文件不存在: {}", artifact_path.display()));
    }

    let normalized_artifact = strip_verbatim_prefix(artifact_path);

    if is_jvm_target(target) {
        verify_jvm_cli_runtime(&normalized_artifact)?;
        return Ok(());
    }

    // `WASI` 目标字符串形如 `wasm32-unknown-wasi-wasi`，用 `contains("wasi")` 检测。
    // 注意：必须先检查 `wasi` 再检查 `wasm`，因为 `WASI` 字符串也包含 `wasm`。
    if target.contains("wasi") {
        verify_wasi_cli_runtime(&normalized_artifact, target)?;
        return Ok(());
    }

    if target.contains("wasm") {
        // WASM 目标（非 WASI）：通过 node 运行 .mjs 启动壳。
        let launcher_path = normalized_artifact.with_extension("mjs");
        if !launcher_path.exists() {
            return Err(miette!("WASM 产物缺少启动壳: {}", launcher_path.display()));
        }

        if !source_file.exists() {
            return Err(miette!("源文件不存在: {}", source_file.display()));
        }

        let mut cmd = Command::new("node");
        // 诚实自举：传递源文件路径，而非固定输入值。
        // 微编译器通过 read_source_byte 导入读取源文件内容。
        cmd.arg(&launcher_path).arg(source_file);

        // 如果有输出路径参数，传递给启动壳用于收集输出字节。
        if let Some(output) = output_path {
            cmd.arg(output);
        }

        println!("verify: executing {:?}", cmd);
        let output = cmd.output().into_diagnostic().map_err(|error| error.wrap_err("执行 node 启动壳失败"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(miette!(
                r#"WASM 产物运行失败
stdout: {}
stderr: {}"#,
                stdout.trim(),
                stderr.trim()
            ));
        }

        // 如果有输出路径，验证输出文件已生成。
        if let Some(output) = output_path {
            if !output.exists() {
                return Err(miette!("WASM 运行成功但未产出输出文件: {}", output.display()));
            }
            println!("  WASM 输出: {} ({} 字节)", output.display(), fs::metadata(output).map(|m| m.len()).unwrap_or(0));
        }

        return Ok(());
    }

    // CLR 目标：执行 .exe 并校验 `--version` / `--help`（与 bootstrap-clr.mjs 对齐）。
    verify_clr_cli_runtime(&normalized_artifact)?;
    Ok(())
}

/// 校验 CLR 产物可响应最小 CLI 契约。
fn verify_clr_cli_runtime(artifact_path: &Path) -> Result<()> {
    let path_str = artifact_path.to_str().ok_or_else(|| miette!("路径包含非 UTF-8 字符"))?;
    let artifact_dir = artifact_path.parent().unwrap_or_else(|| Path::new("."));

    let version_status = run_dotnet_cli(path_str, &["--version"], artifact_dir).map_err(|error| error.wrap_err("执行 v1 --version 失败"))?;
    if !version_status.success() {
        return Err(miette!("v1 --version 返回非零退出码: {} (code={})", version_status, version_status.code().unwrap_or(-1)));
    }

    let help_status = run_dotnet_cli(path_str, &["--help"], artifact_dir).map_err(|error| error.wrap_err("执行 v1 --help 失败"))?;
    if !help_status.success() {
        return Err(miette!("v1 --help 返回非零退出码: {} (code={})", help_status, help_status.code().unwrap_or(-1)));
    }

    Ok(())
}

/// 校验 WASI 产物经 wasmtime 可响应最小 CLI 契约（`--version` / `--help`）。
fn verify_wasi_cli_runtime(artifact_path: &Path, target: &str) -> Result<()> {
    let version = run_wasmtime_cli(artifact_path, target, &["--version"])?;
    if !version.status.success() {
        return Err(miette!(
            r#"v1 --version 失败
stdout: {}
stderr: {}"#,
            String::from_utf8_lossy(&version.stdout).trim(),
            String::from_utf8_lossy(&version.stderr).trim()
        ));
    }
    println!("  v1 --version: {}", String::from_utf8_lossy(&version.stdout).trim());

    let help = run_wasmtime_cli(artifact_path, target, &["--help"])?;
    if !help.status.success() {
        return Err(miette!(
            r#"v1 --help 失败
stdout: {}
stderr: {}"#,
            String::from_utf8_lossy(&help.stdout).trim(),
            String::from_utf8_lossy(&help.stderr).trim()
        ));
    }
    println!("  v1 --help: ok");
    Ok(())
}

/// 运行 `wasmtime run -W gc [-S p3] <artifact> -- <args…>`；若本机未启用 wmemcheck 则不再强求。
fn run_wasmtime_cli(artifact_path: &Path, target: &str, guest_args: &[&str]) -> Result<std::process::Output> {
    let wasmtime = find_wasmtime_cli()?;
    let mut cmd = Command::new(&wasmtime);
    cmd.arg("run");
    cmd.arg("-W");
    cmd.arg("gc");
    cmd.arg("-W");
    cmd.arg("max-memory-size=16777216");
    if target.contains("wasip3") {
        cmd.arg("-S");
        cmd.arg("p3");
    }
    cmd.arg(artifact_path);
    if !guest_args.is_empty() {
        cmd.arg("--");
        for arg in guest_args {
            cmd.arg(arg);
        }
    }
    println!("verify: executing {:?}", cmd);
    cmd.output().into_diagnostic().map_err(|error| error.wrap_err("执行本机 wasmtime 失败"))
}

/// 校验 JVM 产物可被本机 `java -jar` 加载并响应最小 CLI 契约。
fn verify_jvm_cli_runtime(artifact_path: &Path) -> Result<()> {
    let path_str = artifact_path.to_str().ok_or_else(|| miette!("路径包含非 UTF-8 字符"))?;
    let artifact_dir = artifact_path.parent().unwrap_or_else(|| Path::new("."));

    let version_status = run_java_jar_cli(path_str, &["--version"], artifact_dir).map_err(|error| error.wrap_err("执行 v1 --version 失败"))?;
    if !version_status.success() {
        return Err(miette!("v1 --version 返回非零退出码: {} (code={})", version_status, version_status.code().unwrap_or(-1)));
    }

    let help_status = run_java_jar_cli(path_str, &["--help"], artifact_dir).map_err(|error| error.wrap_err("执行 v1 --help 失败"))?;
    if !help_status.success() {
        return Err(miette!("v1 --help 返回非零退出码: {} (code={})", help_status, help_status.code().unwrap_or(-1)));
    }

    Ok(())
}

/// 通过 `dotnet exec` 运行 CLR 产物。
fn run_dotnet_cli(artifact: &str, args: &[&str], cwd: &Path) -> Result<ExitStatus> {
    let output = Command::new("dotnet")
        .arg("exec")
        .arg(artifact)
        .args(args)
        .current_dir(cwd)
        .output()
        .into_diagnostic()
        .map_err(|error| error.wrap_err("无法执行 dotnet"))?;
    Ok(output.status)
}

/// 通过本机 `java -jar` 运行 JVM 产物。
fn run_java_jar_cli(artifact: &str, args: &[&str], cwd: &Path) -> Result<ExitStatus> {
    let output = Command::new("java")
        .arg("-jar")
        .arg(artifact)
        .args(args)
        .current_dir(cwd)
        .output()
        .into_diagnostic()
        .map_err(|error| error.wrap_err("无法执行 java"))?;
    Ok(output.status)
}

/// 运行 CLI 程序并返回退出状态。
#[allow(dead_code)]
fn run_cli_program(program: &str, args: &[&str]) -> Result<ExitStatus> {
    let output =
        Command::new(program).args(args).output().into_diagnostic().map_err(|error| error.wrap_err(format!("无法执行 {}", program)))?;

    Ok(output.status)
}

fn compiler_command(seed: &Path) -> Command {
    if is_managed_jvm_artifact(seed) {
        let mut cmd = Command::new("java");
        cmd.arg("-jar").arg(seed);
        cmd
    }
    else if is_managed_clr_artifact(seed) {
        let mut cmd = Command::new("dotnet");
        cmd.arg("exec").arg(seed);
        cmd
    }
    else {
        Command::new(seed)
    }
}

fn is_jvm_target(target: &str) -> bool {
    let lowered = target.to_ascii_lowercase();
    lowered == "jvm" || lowered.starts_with("jvm-") || lowered.contains("-jvm") || lowered.contains("openjdk")
}

fn is_managed_jvm_artifact(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
}

fn is_managed_clr_artifact(path: &Path) -> bool {
    let runtimeconfig = path.with_extension("runtimeconfig.json");
    if runtimeconfig.exists() {
        return true;
    }
    // Some hosts use `name.runtimeconfig.json` while path is `name.exe`.
    let alt = format!("{}.runtimeconfig.json", path.file_stem().and_then(|s| s.to_str()).unwrap_or_default());
    path.parent().map(|p| p.join(&alt).exists()).unwrap_or(false)
}

fn resolve_manifest_artifact(output_dir: &Path) -> Result<PathBuf> {
    let manifest = ExecutionManifest::read_from_output_dir(output_dir)?
        .ok_or_else(|| miette!("缺少 execution manifest：{}", output_dir.join("run-contracts.txt").display()))?;
    let entries = manifest.run_contracts.iter().map(|contract| contract.physical_entry.as_str()).collect::<Vec<_>>();
    let (artifact, _) =
        select_artifact(output_dir, entries.into_iter(), None).map_err(|error| miette!("execution manifest 产物合同无效：{error}"))?;
    Ok(artifact)
}

fn compare_clr_contracts(v1_dir: &Path, v2_dir: &Path) -> Result<()> {
    let v1 = resolve_manifest_artifact(v1_dir)?;
    let v2 = resolve_manifest_artifact(v2_dir)?;
    compare_artifacts(&v1, &v2)
}

fn compare_jvm_contracts(v1_dir: &Path, v2_dir: &Path) -> Result<()> {
    let v1 = resolve_manifest_artifact(v1_dir)?;
    let v2 = resolve_manifest_artifact(v2_dir)?;
    compare_artifacts(&v1, &v2)
}
fn candidate_command_paths(dir: &Path, command: &str, extensions: &[String]) -> Vec<PathBuf> {
    let base = dir.join(command);
    if Path::new(command).extension().is_some() {
        return vec![base];
    }

    let mut candidates = Vec::with_capacity(1 + extensions.len());
    candidates.push(base.clone());
    for ext in extensions {
        candidates.push(dir.join(format!("{command}{ext}")));
    }
    candidates
}

fn executable_extensions() -> Vec<String> {
    if cfg!(windows) {
        env::var("PATHEXT")
            .ok()
            .map(|value| value.split(';').filter(|item| !item.is_empty()).map(|item| item.to_ascii_lowercase()).collect())
            .unwrap_or_else(|| vec![".exe".to_string(), ".cmd".to_string(), ".bat".to_string(), ".com".to_string()])
    }
    else {
        Vec::new()
    }
}

/// 比对两个产物文件的字节一致性。
///
/// 诚实自举要求 v1 和 v2 是同一份源码两次编译的产物，应字节一致。
/// 若不一致，输出首个差异位置和上下文。
fn compare_artifacts(v1_path: &Path, v2_path: &Path) -> Result<()> {
    if !v1_path.exists() {
        return Err(miette!("v1 产物不存在: {}", v1_path.display()));
    }
    if !v2_path.exists() {
        return Err(miette!("v2 产物不存在: {}", v2_path.display()));
    }

    let v1_bytes = fs::read(v1_path).into_diagnostic().map_err(|error| error.wrap_err("读取 v1 失败"))?;
    let v2_bytes = fs::read(v2_path).into_diagnostic().map_err(|error| error.wrap_err("读取 v2 失败"))?;

    if v1_bytes == v2_bytes {
        println!("  v1/v2 字节一致 ({} bytes)", v1_bytes.len());
        return Ok(());
    }

    // 二进制不一致，报告差异统计。
    let min_len = v1_bytes.len().min(v2_bytes.len());
    let mut diff_pos = None;
    for i in 0..min_len {
        if v1_bytes[i] != v2_bytes[i] {
            diff_pos = Some(i);
            break;
        }
    }

    match diff_pos {
        Some(pos) => Err(miette!(
            r#"v1/v2 比对失败: 二进制不一致
首次差异位置: 0x{pos:X} (v1=0x{v1:02X}, v2=0x{v2:02X})
v1 长度: {v1_len}, v2 长度: {v2_len}"#,
            pos = pos,
            v1 = v1_bytes[pos],
            v2 = v2_bytes[pos],
            v1_len = v1_bytes.len(),
            v2_len = v2_bytes.len()
        )),
        None => Err(miette!("v1/v2 比对失败: 长度不一致 (v1: {}, v2: {})", v1_bytes.len(), v2_bytes.len())),
    }
}

fn wasm_family_extension(target: &str) -> &'static str {
    if target.contains("wasi") { "wasi" } else { "wasm" }
}

/// 获取编译器源文件路径。
///
/// 诚实自举要求 v1 实际读取源文件内容，而非使用固定输入值。
/// CLR 目标通过 `legion build <project_dir>` 收集项目源码闭包；
/// WASM 目标仍使用 `<project_dir>/source/main.v` 作为运行输入。
fn get_compiler_source_path(project_dir: &Path) -> PathBuf {
    project_dir.join("source").join("main.v")
}
