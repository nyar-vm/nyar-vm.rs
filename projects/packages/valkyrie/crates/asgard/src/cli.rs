//! `asgard` CLI：`asgard build` / `asgard dev` / `asgard pack`。

use std::{path::PathBuf, process::ExitCode};

use clap::{Args, Parser, Subcommand, ValueEnum};
use miette::{Result, miette};
use nyar_language::formatter::{FormatCliOptions, format_options_from_cli, normalize_extensions, report_format_cli_with, run_format_cli_with};
use rusty_valkyrie::CanonicalTarget;

use crate::{
    CompileOptions, HostArtifactKind, HostPlatform, PackOptions, PackTarget, compile_voa_project,
    config::VoaConfig,
    dev_server::{DevServerOptions, generate_hmr_client_script, run_dev_server},
    pack_voa_delivery,
};

/// `asgard build` / `asgard dev` 共用参数。
#[derive(Debug, Clone, Args)]
pub struct AsgardBuildArgs {
    /// 项目目录。
    #[arg(value_name = "project-dir", default_value = ".")]
    pub project_dir: PathBuf,
    /// 输出目录。
    #[arg(short = 'o', long = "output")]
    pub output_dir: Option<PathBuf>,
    /// 覆盖 `asgard.config.v` 中的 `target`（默认使用配置值）。
    #[arg(long)]
    pub target: Option<String>,
}

/// `asgard pack` 参数。
#[derive(Debug, Clone, Args)]
pub struct AsgardPackArgs {
    /// 交付目标。
    #[arg(long, value_enum)]
    pub target: AsgardPackTargetArg,
    /// 项目目录或 dist。
    #[arg(long, default_value = ".")]
    pub input: PathBuf,
    /// 输出目录。
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// 小程序/小游戏项目名。
    #[arg(long)]
    pub project_name: Option<String>,
    /// 小游戏 WASM 文件名。
    #[arg(long)]
    pub wasm: Option<String>,
}

/// `asgard pack` 的打包目标参数。
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum AsgardPackTargetArg {
    /// Android APK。
    Apk,
    /// iOS IPA。
    Ipa,
    /// 小程序。
    #[value(name = "mini-program")]
    MiniProgram,
    /// 小游戏。
    #[value(name = "mini-game")]
    MiniGame,
}

impl From<AsgardPackTargetArg> for PackTarget {
    fn from(value: AsgardPackTargetArg) -> Self {
        match value {
            AsgardPackTargetArg::Apk => PackTarget::Apk,
            AsgardPackTargetArg::Ipa => PackTarget::Ipa,
            AsgardPackTargetArg::MiniProgram => PackTarget::MiniProgram,
            AsgardPackTargetArg::MiniGame => PackTarget::MiniGame,
        }
    }
}

/// `asgard add` / `asgard remove` 参数。
#[derive(Debug, Clone, Args)]
pub struct AsgardDepArgs {
    /// 依赖包名。
    pub name: String,
    /// 版本（add 时使用）。
    #[arg(default_value = "0.1.0")]
    pub version: String,
    /// 项目目录。
    #[arg(value_name = "project-dir", default_value = ".")]
    pub project_dir: PathBuf,
}

/// `asgard publish` 参数。
#[derive(Debug, Clone, Args)]
pub struct AsgardPublishArgs {
    /// 发布目标。
    #[arg(long, value_enum)]
    pub target: AsgardPublishTargetArg,
    /// 项目目录或 dist。
    #[arg(long, default_value = ".")]
    pub input: PathBuf,
    /// 输出目录。
    #[arg(long)]
    pub output: Option<PathBuf>,
}

/// `asgard publish` 的发布目标参数。
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum AsgardPublishTargetArg {
    /// Android APK。
    Apk,
    /// iOS IPA。
    Ipa,
    /// 小程序。
    #[value(name = "mini-program")]
    MiniProgram,
    /// Web 产物。
    Web,
}

impl From<AsgardPublishTargetArg> for crate::PublishTarget {
    fn from(value: AsgardPublishTargetArg) -> Self {
        match value {
            AsgardPublishTargetArg::Apk => Self::Apk,
            AsgardPublishTargetArg::Ipa => Self::Ipa,
            AsgardPublishTargetArg::MiniProgram => Self::MiniProgram,
            AsgardPublishTargetArg::Web => Self::Web,
        }
    }
}

/// `asgard fmt` / `asgard format` 参数。
#[derive(Debug, Clone, Args)]
pub struct AsgardFmtArgs {
    /// 要格式化的文件或目录（默认当前目录）。
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,
    /// 仅检查是否已格式化，不写盘；有差异时退出码非 0。
    #[arg(long)]
    pub check: bool,
    /// 忽略 `.editorconfig`，使用 CLI 指定的缩进/行宽。
    #[arg(long)]
    pub no_editorconfig: bool,
    /// 打印每个被改写的文件。
    #[arg(short, long)]
    pub verbose: bool,
    /// 有文件因解析错误被跳过时以非 0 退出。
    #[arg(long)]
    pub fail_on_skip: bool,
    /// 缩进宽度（空格数）；仅在与 `--no-editorconfig` 联用时生效。
    #[arg(long = "indent-width")]
    pub indent_width: Option<usize>,
    /// 折行行宽；仅在与 `--no-editorconfig` 联用时生效。
    #[arg(long = "max-width")]
    pub max_width: Option<usize>,
    /// 仅处理指定扩展名（可重复，如 `--ext awsl --ext v`）。
    #[arg(long = "ext", value_name = "EXT")]
    pub extensions: Vec<String>,
    /// 仅格式化 AWSL（`.awsl`）；等价于 `--ext awsl`。
    #[arg(long)]
    pub awsl_only: bool,
}

impl AsgardFmtArgs {
    fn to_cli_options(&self) -> FormatCliOptions {
        let mut extensions = if self.extensions.is_empty() { None } else { Some(normalize_extensions(&self.extensions)) };
        if self.awsl_only {
            extensions = Some(vec!["awsl".into()]);
        }
        FormatCliOptions {
            paths: self.paths.clone(),
            check: self.check,
            use_editorconfig: !self.no_editorconfig,
            verbose: self.verbose,
            fail_on_skip: self.fail_on_skip,
            format: format_options_from_cli(self.indent_width, self.max_width),
            extensions,
        }
    }
}

/// `asgard plan`：校验 deploy profile 并打印制品矩阵；或列出 profiles 目录。
#[derive(Debug, Clone, Args)]
pub struct AsgardPlanArgs {
    /// Deploy profile 路径（`.von`）。与 `--list` 互斥其一必填。
    #[arg(long = "profile", value_name = "PROFILE")]
    pub profile: Option<PathBuf>,
    /// 列出目录下的 `*.von` profile（只列名，不校验内容）。
    #[arg(long = "list", value_name = "DIR")]
    pub list: Option<PathBuf>,
}

/// `asgard` 子命令。
#[derive(Debug, Subcommand)]
pub enum AsgardCommands {
    /// 构建 Asgard 项目（Release / 配置中的 `build.mode`）。
    Build(AsgardBuildArgs),
    /// 开发模式构建（含 HMR 开发服务器，browser 平台）。
    Dev(AsgardBuildArgs),
    /// 按交付目标组装交付物（apk / ipa / mini-program / mini-game）。
    Pack(AsgardPackArgs),
    /// 校验 deploy profile 并打印将构建的制品矩阵（不编排构建）。
    Plan(AsgardPlanArgs),
    /// 添加 legion.von 依赖。
    Add(AsgardDepArgs),
    /// 移除 legion.von 依赖。
    Remove(AsgardDepArgs),
    /// 发布到商店 / CDN（先 pack 再对接平台 CLI）。
    Publish(AsgardPublishArgs),
    /// 格式化 `.v` / `.vx` / `.von` / `.awsl`（与 `legion fmt` 同引擎）。
    #[command(visible_alias = "format")]
    Fmt(AsgardFmtArgs),
}

fn compile_with_args(args: &AsgardBuildArgs) -> Result<crate::CompileReport> {
    let target = match &args.target {
        Some(value) => Some(CanonicalTarget::parse(value).map_err(|error| miette::miette!("{error}"))?),
        None => None,
    };
    compile_voa_project(&CompileOptions { project_dir: args.project_dir.clone(), output_dir: args.output_dir.clone(), target })
}

fn print_build_report(report: &crate::CompileReport, label: &str) {
    println!("{label} complete");
    println!("output: {}", report.output_dir.display());
    println!("components: {}", report.component_count);
    println!("js files: {}", report.js_file_count);
    println!("wasm: {}", if report.wasm_built { "yes" } else { "no" });
    if report.host_logic_built {
        let kind = report
            .host_artifact_kind
            .map(|k| match k {
                HostArtifactKind::Wasm => "wasm",
                HostArtifactKind::JvmClass => "jvm-class",
                HostArtifactKind::NativeExecutable => "native-exe",
            })
            .unwrap_or("unknown");
        println!("host logic: yes ({kind})");
    }
    else if !matches!(report.platform, HostPlatform::Browser) {
        println!("host logic: no");
    }
}

/// 执行 `asgard build`。
pub fn run_build(args: &AsgardBuildArgs) -> Result<ExitCode> {
    let report = compile_with_args(args)?;
    print_build_report(&report, "asgard build");
    Ok(ExitCode::SUCCESS)
}

/// 执行 `asgard dev`。
pub fn run_dev(args: &AsgardBuildArgs) -> Result<ExitCode> {
    let config = VoaConfig::load(&args.project_dir)?;
    let report = compile_with_args(args)?;
    print_build_report(&report, "asgard dev");

    if !matches!(report.platform, HostPlatform::Browser) {
        println!("hint: 原生平台请使用 `asgard dev --watch` 模式：监视源码并重建 dist");
        if config.hot_reload.enabled {
            let server = DevServerOptions {
                project_dir: args.project_dir.clone(),
                dist_dir: report.output_dir.clone(),
                host: "127.0.0.1".into(),
                port: config.hot_reload.port,
                watch_dirs: config.hot_reload.watch.iter().map(|s| s.trim_end_matches('/').to_string()).collect(),
                ignore_dirs: config.hot_reload.ignore.clone(),
                debounce_ms: config.hot_reload.debounce,
            };
            let compile = CompileOptions {
                project_dir: args.project_dir.clone(),
                output_dir: Some(report.output_dir.clone()),
                target: match &args.target {
                    Some(value) => Some(CanonicalTarget::parse(value).map_err(|error| miette::miette!("{error}"))?),
                    None => None,
                },
            };
            println!("hint: 重建后请重新安装 APK/IPA 或运行 dist/{{platform}}/ 可执行文件");
            run_dev_server(&server, &compile)?;
        }
        return Ok(ExitCode::SUCCESS);
    }

    if config.hot_reload.enabled {
        inject_hmr_client(&report.output_dir)?;
        let server = DevServerOptions {
            project_dir: args.project_dir.clone(),
            dist_dir: report.output_dir.clone(),
            host: "127.0.0.1".into(),
            port: config.hot_reload.port,
            watch_dirs: config.hot_reload.watch.iter().map(|s| s.trim_end_matches('/').to_string()).collect(),
            ignore_dirs: config.hot_reload.ignore.clone(),
            debounce_ms: config.hot_reload.debounce,
        };
        let compile = CompileOptions {
            project_dir: args.project_dir.clone(),
            output_dir: Some(report.output_dir.clone()),
            target: match &args.target {
                Some(value) => Some(CanonicalTarget::parse(value).map_err(|error| miette::miette!("{error}"))?),
                None => None,
            },
        };
        run_dev_server(&server, &compile)?;
    }
    else {
        println!("hint: hot_reload.enabled=false；在 dist 目录启动静态服务器预览");
    }
    Ok(ExitCode::SUCCESS)
}

fn inject_hmr_client(dist_dir: &std::path::Path) -> Result<()> {
    use miette::IntoDiagnostic;
    use std::fs;
    let index_path = dist_dir.join("index.html");
    if !index_path.exists() {
        return Ok(());
    }
    let mut html = fs::read_to_string(&index_path).into_diagnostic()?;
    let script = generate_hmr_client_script();
    let tag = format!("<script>{script}</script>");
    if !html.contains("/__asgard/hmr") {
        if html.contains("</body>") {
            html = html.replace("</body>", &format!("{tag}</body>"));
        }
        else {
            html.push_str(&tag);
        }
        fs::write(&index_path, html).into_diagnostic()?;
    }
    Ok(())
}

/// 执行 `asgard pack`。
pub fn run_pack(args: &AsgardPackArgs) -> Result<ExitCode> {
    let report = pack_voa_delivery(&PackOptions {
        input: args.input.clone(),
        output: args.output.clone(),
        target: args.target.into(),
        project_name: args.project_name.clone(),
        wasm_name: args.wasm.clone(),
    })?;
    println!("asgard pack complete");
    println!("{}", report.message);
    println!("artifact: {}", report.artifact_path.display());
    Ok(ExitCode::SUCCESS)
}

/// 执行 `asgard plan`（deploy profile 校验 + 矩阵打印，或列出 profiles）。
pub fn run_plan(args: &AsgardPlanArgs) -> Result<ExitCode> {
    match (&args.list, &args.profile) {
        (Some(dir), None) => {
            let names = crate::deploy::list_deploy_profiles(dir)?;
            println!("asgard plan --list {}", dir.display());
            if names.is_empty() {
                println!("(no *.von profiles)");
            }
            else {
                for name in names {
                    println!("{name}");
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        (None, Some(profile)) => {
            let plan = crate::deploy::DeployProfile::load(profile)?;
            crate::deploy::print_deploy_plan(&plan);
            Ok(ExitCode::SUCCESS)
        }
        (Some(_), Some(_)) => Err(miette!("asgard plan: 请只指定 --profile 或 --list 之一")),
        (None, None) => Err(miette!("asgard plan: 需要 --profile <PATH> 或 --list <DIR>")),
    }
}

/// 执行 `asgard fmt` / `asgard format`（与 `legion fmt` 共用 `nyar_language::formatter`）。
pub fn run_fmt(args: &AsgardFmtArgs) -> Result<ExitCode> {
    let options = args.to_cli_options();
    let report = run_format_cli_with(&options).map_err(|error| miette!("{error}"))?;
    if report_format_cli_with(&report, &options) { Ok(ExitCode::FAILURE) } else { Ok(ExitCode::SUCCESS) }
}

/// 分发 `asgard` 子命令。
pub fn run(command: &AsgardCommands) -> Result<ExitCode> {
    match command {
        AsgardCommands::Build(args) => run_build(args),
        AsgardCommands::Dev(args) => run_dev(args),
        AsgardCommands::Pack(args) => run_pack(args),
        AsgardCommands::Plan(args) => run_plan(args),
        AsgardCommands::Add(args) => {
            crate::deps::add_dependency(&args.project_dir, &args.name, &args.version)?;
            println!("added dependency {} = {}", args.name, args.version);
            Ok(ExitCode::SUCCESS)
        }
        AsgardCommands::Remove(args) => {
            crate::deps::remove_dependency(&args.project_dir, &args.name)?;
            println!("removed dependency {}", args.name);
            Ok(ExitCode::SUCCESS)
        }
        AsgardCommands::Publish(args) => {
            let report = crate::publish_voa(&args.input, args.target.into(), args.output.as_deref())?;
            println!("asgard publish: {}", report.message);
            println!("artifact: {}", report.artifact_path.display());
            Ok(ExitCode::SUCCESS)
        }
        AsgardCommands::Fmt(args) => run_fmt(args),
    }
}

#[derive(Debug, Parser)]
#[command(name = "asgard", version, about = "Asgard — 跨平台 GUI 应用框架 CLI")]
struct AsgardCli {
    #[command(subcommand)]
    command: AsgardCommands,
}

/// 解析当前进程 `argv` 并运行 Asgard CLI（供 `vcc-napi` / `vcc-wasm` 宿主调用）。
pub fn run_from_env() -> Result<ExitCode> {
    let cli = AsgardCli::parse();
    run(&cli.command)
}
