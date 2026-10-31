//! `asgard` CLI：`asgard build` / `asgard dev` / `asgard pack`。
use crate::asgard::error::{AsgardError, AsgardResult};
use nyar_format::formatter::{FormatCliOptions, format_options_from_cli, normalize_extensions, report_format_cli_with, run_format_cli_with};
use std::{path::PathBuf, process::ExitCode};

use crate::asgard::{
    PackOptions, PackTarget, pack_voa_delivery,
    static_site::{StaticSiteOptions, build_static_site},
};

/// `asgard build` 的交付方式。
#[derive(Debug, Clone, Default)]
pub enum AsgardDelivery {
    /// 完整应用构建（默认）：配置求值（C0/C2）未闭合，当前为失败闭合诊断。
    #[default]
    App,
    /// 静态站点交付：显式 AWSL 根组件入口，渲染带样式的 `index.html`，不进入应用管线。
    Static {
        /// 入口根组件文件（相对 `project_dir`，`.w` 或 `.awsl`，须显式给出）。
        entry: PathBuf,
        /// 页面 `<title>` 文本；`None` 时使用根组件名（入口文件 stem）。
        title: Option<String>,
    },
}

/// `asgard build` / `asgard dev` 共用参数。
#[derive(Debug, Clone)]
pub struct AsgardBuildArgs {
    /// 项目目录。
    pub project_dir: PathBuf,
    /// 输出目录。
    pub output_dir: Option<PathBuf>,
    /// 覆盖 `asgard.config.v` 中的 `target`（默认使用配置值）。
    pub target: Option<String>,
    /// 交付方式；默认 `App` 保持既有行为，`Static` 走静态站点编排。
    pub delivery: AsgardDelivery,
}

/// `asgard pack` 参数。
#[derive(Debug, Clone)]
pub struct AsgardPackArgs {
    /// 交付目标。
    pub target: AsgardPackTargetArg,
    /// 项目目录或 dist。
    pub input: PathBuf,
    /// 输出目录。
    pub output: Option<PathBuf>,
    /// 小程序/小游戏项目名。
    pub project_name: Option<String>,
    /// 小游戏 WASM 文件名。
    pub wasm: Option<String>,
}

/// `asgard pack` 的打包目标参数。
#[derive(Debug, Clone, Copy)]
pub enum AsgardPackTargetArg {
    /// Android APK。
    Apk,
    /// iOS IPA。
    Ipa,
    /// 小程序（`mini-program`）。
    MiniProgram,
    /// 小游戏（`mini-game`）。
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

/// `asgard publish` 参数。
#[derive(Debug, Clone)]
pub struct AsgardPublishArgs {
    /// 发布目标。
    pub target: AsgardPublishTargetArg,
    /// 项目目录或 dist。
    pub input: PathBuf,
    /// 输出目录。
    pub output: Option<PathBuf>,
}

/// `asgard publish` 的发布目标参数。
#[derive(Debug, Clone, Copy)]
pub enum AsgardPublishTargetArg {
    /// Android APK。
    Apk,
    /// iOS IPA。
    Ipa,
    /// 小程序（`mini-program`）。
    MiniProgram,
    /// Web 产物。
    Web,
}

impl From<AsgardPublishTargetArg> for crate::asgard::PublishTarget {
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
#[derive(Debug, Clone)]
pub struct AsgardFmtArgs {
    /// 要格式化的文件或目录（默认当前目录）。
    pub paths: Vec<PathBuf>,
    /// 仅检查是否已格式化，不写盘；有差异时退出码非 0。
    pub check: bool,
    /// 忽略 `.editorconfig`，使用 CLI 指定的缩进/行宽。
    pub no_editorconfig: bool,
    /// 打印每个被改写的文件。
    pub verbose: bool,
    /// 有文件因解析错误被跳过时以非 0 退出。
    pub fail_on_skip: bool,
    /// 缩进宽度（空格数）；仅在与 `--no-editorconfig` 联用时生效。
    pub indent_width: Option<usize>,
    /// 折行行宽；仅在与 `--no-editorconfig` 联用时生效。
    pub max_width: Option<usize>,
    /// 仅处理指定扩展名（可重复，如 `--ext awsl --ext v`）。
    pub extensions: Vec<String>,
    /// 仅格式化 AWSL（`.awsl`）；等价于 `--ext awsl`。
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
#[derive(Debug, Clone)]
pub struct AsgardPlanArgs {
    /// Deploy profile 路径（`.von`）。与 `--list` 互斥其一必填。
    pub profile: Option<PathBuf>,
    /// 列出目录下的 `*.von` profile（只列名，不校验内容）。
    pub list: Option<PathBuf>,
}

/// `asgard` 子命令。
#[derive(Debug)]
pub enum AsgardCommands {
    /// 构建 Asgard 项目；`App` 交付等待配置求值接入（失败闭合诊断），`Static` 交付已可用。
    Build(AsgardBuildArgs),
    /// 开发模式（含 HMR 开发服务器）：配置求值未闭合，当前为失败闭合诊断。
    Dev(AsgardBuildArgs),
    /// 按交付目标组装交付物（apk / ipa / mini-program / mini-game）。
    Pack(AsgardPackArgs),
    /// 校验 deploy profile 并打印将构建的制品矩阵（不编排构建）。
    Plan(AsgardPlanArgs),
    /// 发布到商店 / CDN（先 pack 再对接平台 CLI）。
    Publish(AsgardPublishArgs),
    /// 格式化 `.v` / `.vx` / `.von` / `.awsl`（与 `legion fmt` 同引擎）。
    Fmt(AsgardFmtArgs),
}

/// 执行 `asgard build`。
pub fn run_build(args: &AsgardBuildArgs) -> AsgardResult<ExitCode> {
    match &args.delivery {
        AsgardDelivery::App => {
            return Err(AsgardError::msg(report!(
                "asgard build (app): 配置求值（C0/C2）未闭合，应用管线不再读取或解析 `asgard.config.v`；首轮只交付 `build --static` 显式入口，App 构建等待配置求值接入（项目：{}）",
                args.project_dir.display()
            )));
        }
        AsgardDelivery::Static { entry, title } => {
            let report = build_static_site(&StaticSiteOptions {
                project_dir: args.project_dir.clone(),
                entry: entry.clone(),
                output_dir: args.output_dir.clone(),
                title: title.clone(),
                base: "/".to_string(),
            })?;
            println!("asgard build (static) complete");
            println!("output: {}", report.output_dir.display());
            println!("page: {}", report.page_path.display());
            println!("component: {}", report.component_name);
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// 执行 `asgard dev`。
pub fn run_dev(args: &AsgardBuildArgs) -> AsgardResult<ExitCode> {
    Err(AsgardError::msg(report!(
        "asgard dev: 配置求值（C0/C2）未闭合，首轮只交付 `build --static` 显式入口；dev 等待配置求值接入（项目：{}）",
        args.project_dir.display()
    )))
}

/// 执行 `asgard pack`。
pub fn run_pack(args: &AsgardPackArgs) -> AsgardResult<ExitCode> {
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
pub fn run_plan(args: &AsgardPlanArgs) -> AsgardResult<ExitCode> {
    match (&args.list, &args.profile) {
        (Some(dir), None) => {
            let names = crate::asgard::deploy::list_deploy_profiles(dir)?;
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
            let plan = crate::asgard::deploy::DeployProfile::load(profile)?;
            crate::asgard::deploy::print_deploy_plan(&plan);
            Ok(ExitCode::SUCCESS)
        }
        (Some(_), Some(_)) => Err(AsgardError::msg(report!("asgard plan: 请只指定 --profile 或 --list 之一"))),
        (None, None) => Err(AsgardError::msg(report!("asgard plan: 需要 --profile <PATH> 或 --list <DIR>"))),
    }
}

/// 执行 `asgard fmt` / `asgard format`（与 `legion fmt` 共用 `nyar_format::formatter`）。
pub fn run_fmt(args: &AsgardFmtArgs) -> AsgardResult<ExitCode> {
    let options = args.to_cli_options();
    let report = run_format_cli_with(&options).map_err(|error| AsgardError::msg(report!("{error}")))?;
    if report_format_cli_with(&report, &options) { Ok(ExitCode::FAILURE) } else { Ok(ExitCode::SUCCESS) }
}

/// 分发 `asgard` 子命令。
pub fn run(command: &AsgardCommands) -> AsgardResult<ExitCode> {
    // 格式化是平台能力：Valkyrie formatter / printer 由前端在入口注册。
    crate::compiler::services::format_provider::register_format_providers();
    match command {
        AsgardCommands::Build(args) => run_build(args),
        AsgardCommands::Dev(args) => run_dev(args),
        AsgardCommands::Pack(args) => run_pack(args),
        AsgardCommands::Plan(args) => run_plan(args),
        AsgardCommands::Publish(args) => {
            let report = crate::asgard::publish_voa(&args.input, args.target.into(), args.output.as_deref())?;
            println!("asgard publish: {}", report.message);
            println!("artifact: {}", report.artifact_path.display());
            Ok(ExitCode::SUCCESS)
        }
        AsgardCommands::Fmt(args) => run_fmt(args),
    }
}
