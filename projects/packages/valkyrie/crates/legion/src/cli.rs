//! Legion CLI 解析与分发（库内入口）。
//!
//! 原生宿主仅经 `vcc-napi` platform `.node`（`legionRun*` N-API）或 wasm collect 调用；
//! npm 用户入口在 `packages/legion`（`bin/legion.js` → `@valkyrie-language/vcc` 宿主）。

use std::{ffi::OsString, io::Read, process::ExitCode};

use clap::{Parser, Subcommand, error::ErrorKind};
use miette::Report;

use crate::{
    SpyOptions,
    cmds::{
        audit::{AuditArgs, run as run_audit},
        bench::{BenchArgs, run as run_bench},
        bootstrap::{BootstrapArgs, run as run_bootstrap},
        build::{BuildArgs, run as run_build},
        check::{CheckArgs, run as run_check},
        clean::{CleanArgs, run as run_clean},
        cov::{CovArgs, run as run_cov},
        doc::{DocArgs, run as run_doc},
        fmt::{FmtArgs, run as run_fmt},
        install::{AddArgs, InstallArgs, RemoveArgs, UpdateArgs, run as run_install, run_add, run_remove, run_update},
        lint::{LintArgs, run as run_lint},
        login::{LoginArgs, LogoutArgs, WhoamiArgs, run_login, run_logout, run_whoami},
        plan::{PlanArgs, run as run_plan},
        publish::{PublishArgs, run as run_publish},
        registry::{RegistryArgs, run as run_registry},
        run::{RunArgs, run as run_run},
        search::{InfoArgs, SearchArgs, run as run_search, run_info},
        test::{TestArgs, run as run_test},
        vendor::{VendorArgs, run as run_vendor},
    },
    forward::try_forward_companion,
    run_spy,
};

/// VCC / Legion 根命令（seed 二进制名 `vcc`；npm 包装仍可称 `legion`）。
#[derive(Debug, Parser)]
#[command(name = "vcc", version, about = "Valkyrie 工作区命令行入口")]
pub struct LegionCli {
    /// 已解析的 Legion 子命令。
    #[command(subcommand)]
    pub command: LegionCommands,
}

/// Legion 子命令。
#[derive(Debug, Subcommand)]
pub enum LegionCommands {
    /// 构建项目。
    Build(BuildArgs),
    /// 解析 workspace 并输出 build plan（不进入 Compiler）。
    Plan(PlanArgs),
    /// 检查项目可编译性。
    Check(CheckArgs),
    /// 清理构建产物目录。
    Clean(CleanArgs),
    /// 运行项目产物。
    Run(RunArgs),
    /// 诊断目标产物。
    Spy(SpyOptions),
    /// 自举编译链：seed -> v1 -> v2。
    Bootstrap(BootstrapArgs),
    /// 生成项目文档站点（用户文档 + hub）。
    Doc(DocArgs),
    /// 运行测试并生成 HTML 报告。
    Test(TestArgs),
    /// 收集语法覆盖率并生成 HTML 报告。
    #[command(visible_alias = "coverage")]
    Cov(CovArgs),
    /// 运行基准测试并生成 HTML 报告。
    #[command(visible_alias = "benchmark")]
    Bench(BenchArgs),
    /// 格式化代码。
    #[command(visible_alias = "format")]
    Fmt(FmtArgs),
    /// 运行前端语义 lint（不生成后端产物）。
    Lint(LintArgs),
    /// 发布包到注册表。
    Publish(PublishArgs),
    /// 登录注册表（与 npm / deno 等官方 CLI 凭据互通）。
    Login(LoginArgs),
    /// 退出注册表登录。
    Logout(LogoutArgs),
    /// 查看当前登录用户。
    Whoami(WhoamiArgs),
    /// 安装依赖。
    Install(InstallArgs),
    /// 添加依赖。
    Add(AddArgs),
    /// 移除依赖。
    Remove(RemoveArgs),
    /// 更新依赖。
    Update(UpdateArgs),
    /// 搜索注册表中的包。
    Search(SearchArgs),
    /// 查看包信息。
    Info(InfoArgs),
    /// 注册表认证管理。
    Vendor(VendorArgs),
    /// 审计依赖漏洞与许可证。
    Audit(AuditArgs),
    /// 管理注册表源 endpoint。
    Registry(RegistryArgs),
}

/// 在独立大栈线程上解析 `argv` 并运行 Legion CLI（供 `vcc-napi` 原生宿主调用）。
pub fn run_from_env() -> i32 {
    run_with_argv(std::env::args_os().collect())
}

/// 使用显式 `argv` 运行 Legion CLI（首项通常为 `"legion"`）。
pub fn run_with_argv(argv: Vec<OsString>) -> i32 {
    let result = spawn_cli_thread(argv, run_with_argv_inner);
    exit_code_from_result(result)
}

/// 在当前线程解析 `argv` 并运行 Legion CLI（供 N-API 捕获 stdout/stderr 时使用）。
pub fn run_with_argv_in_current_thread(argv: Vec<OsString>) -> i32 {
    exit_code_from_result(run_with_argv_inner(argv))
}

/// 与 [`run_with_argv_captured`] 相同，但不在独立线程中运行（供 N-API 宿主避免 Windows 线程+stdio 死锁）。
pub fn run_with_argv_captured_on_current_thread(argv: Vec<OsString>) -> (i32, String, String) {
    match capture_cli_run(argv, run_with_argv_inner) {
        Ok(CapturedCliRun { code, stdout, stderr }) => (exit_code_from_exit_code(code), stdout, stderr),
        Err(report) => {
            let message = format!("{report:?}");
            (1, String::new(), message)
        }
    }
}

/// 与 [`run_with_argv`] 相同，但在 Legion 工作线程内捕获 stdout/stderr。
pub fn run_with_argv_captured(argv: Vec<OsString>) -> (i32, String, String) {
    let result = spawn_cli_thread(argv, |argv| capture_cli_run(argv, run_with_argv_inner));

    match result {
        Ok(CapturedCliRun { code, stdout, stderr }) => (exit_code_from_exit_code(code), stdout, stderr),
        Err(report) => {
            let message = format!("{report:?}");
            (1, String::new(), message)
        }
    }
}

fn capture_cli_run<F>(argv: Vec<OsString>, run: F) -> Result<CapturedCliRun, Report>
where
    F: FnOnce(Vec<OsString>) -> Result<ExitCode, Report>,
{
    let mut stdout_redirect = gag::BufferRedirect::stdout().map_err(|error| Report::msg(error.to_string()))?;
    let mut stderr_redirect = gag::BufferRedirect::stderr().map_err(|error| Report::msg(error.to_string()))?;
    let code = run(argv)?;
    let mut stdout = String::new();
    let mut stderr = String::new();
    let _ = stdout_redirect.read_to_string(&mut stdout);
    let _ = stderr_redirect.read_to_string(&mut stderr);
    Ok(CapturedCliRun { code, stdout, stderr })
}

struct CapturedCliRun {
    code: ExitCode,
    stdout: String,
    stderr: String,
}

fn spawn_cli_thread<T, F>(argv: Vec<OsString>, run: F) -> Result<T, Report>
where
    T: Send + 'static,
    F: FnOnce(Vec<OsString>) -> Result<T, Report> + Send + 'static,
{
    std::thread::Builder::new()
        .name("legion-main".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || run(argv))
        .expect("failed to spawn legion main thread")
        .join()
        .expect("legion main thread panicked")
}

fn exit_code_from_exit_code(code: ExitCode) -> i32 {
    if code == ExitCode::SUCCESS { 0 } else { 1 }
}

fn run_with_argv_inner(argv: Vec<OsString>) -> Result<ExitCode, Report> {
    if let Some(code) = try_forward_companion(&argv) {
        return Ok(code);
    }
    let cli = match LegionCli::try_parse_from(&argv) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            match error.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand | ErrorKind::DisplayVersion => {
                    return Ok(ExitCode::SUCCESS);
                }
                _ => return Err(Report::msg(error.to_string())),
            }
        }
    };
    dispatch(cli)
}

fn exit_code_from_result(result: Result<ExitCode, Report>) -> i32 {
    match result {
        Ok(code) => exit_code_from_exit_code(code),
        Err(report) => {
            eprintln!("{report:?}");
            1
        }
    }
}

/// 分发已解析的 Legion 子命令。
pub fn dispatch(cli: LegionCli) -> Result<ExitCode, Report> {
    match cli.command {
        LegionCommands::Build(args) => run_build(&args),
        LegionCommands::Plan(args) => run_plan(&args),
        LegionCommands::Check(args) => run_check(&args),
        LegionCommands::Clean(args) => run_clean(&args),
        LegionCommands::Run(args) => run_run(&args),
        LegionCommands::Spy(options) => run_spy(&options),
        LegionCommands::Bootstrap(args) => {
            let result = run_bootstrap(&args)?;
            if result.is_success() {
                println!("自举完成: 成功 {} 个阶段", result.stages_completed.len());
                return Ok(ExitCode::SUCCESS);
            }
            if let Some((stage, report)) = result.failed_stage {
                return Err(report.wrap_err(format!("自举失败于阶段 [{}]", stage)));
            }
            Ok(ExitCode::FAILURE)
        }
        LegionCommands::Doc(args) => run_doc(&args),
        LegionCommands::Test(args) => run_test(&args),
        LegionCommands::Cov(args) => run_cov(&args),
        LegionCommands::Bench(args) => run_bench(&args),
        LegionCommands::Fmt(args) => run_fmt(&args),
        LegionCommands::Lint(args) => run_lint(&args),
        LegionCommands::Publish(args) => run_publish(&args),
        LegionCommands::Login(args) => run_login(&args),
        LegionCommands::Logout(args) => run_logout(&args),
        LegionCommands::Whoami(args) => run_whoami(&args),
        LegionCommands::Install(args) => run_install(&args),
        LegionCommands::Add(args) => run_add(&args),
        LegionCommands::Remove(args) => run_remove(&args),
        LegionCommands::Update(args) => run_update(&args),
        LegionCommands::Search(args) => run_search(&args),
        LegionCommands::Info(args) => run_info(&args),
        LegionCommands::Vendor(args) => run_vendor(&args),
        LegionCommands::Audit(args) => run_audit(&args),
        LegionCommands::Registry(args) => run_registry(&args),
    }
}
