//! Node-API（N-API）原生绑定层：把 `legion` / `asgard` 编译器能力导出给 Node 宿主。
//!
//! 用户面对的 `legion` / `asgard` 命令行在 `packages/legion` 与 `packages/asgard`，
//! 由本 crate 与各 `packages/vcc-*` 平台 collect 组装而成。

#![deny(missing_docs)]

mod host_api;
mod stdio_capture;

pub use host_api::{LegionBuildOptions, LegionBuildOutcome, LegionPlanOptions, LegionPlanOutcome, LegionSemanticGroupItem};

use std::{ffi::OsString, process::ExitCode, thread};

use legion::{SpyMode, SpyOptions, SpyTargetOptions, SpyTargetPlatform, cli, run_spy};
use napi_derive::napi;

pub use asgard;
pub use legion;

/// `legion run` / `legion spy` 等 CLI 子进程等价结果。
#[napi(object)]
pub struct LegionRunOutcome {
    /// 进程退出码（0 表示成功）。
    pub status: u32,
    /// 捕获的标准输出。
    pub stdout: String,
    /// 捕获的标准错误。
    pub stderr: String,
}

/// `legion spy` 各模式共享的目标参数（与 Rust `SpyTargetOptions` 对齐）。
#[napi(object)]
pub struct SpyTargetRunOptions {
    /// 目标文件路径或项目名。
    pub input: Option<String>,
    /// 函数索引或函数名（wasm/lir/mir）。
    pub func: Option<String>,
    /// 方法名（jvm/clr）。
    pub method: Option<String>,
    /// 绝对偏移量（wasm）。
    pub offset: Option<i64>,
    /// 是否列出所有函数/方法。
    pub list: Option<bool>,
    /// 错误点上下文行数。
    pub context: Option<u32>,
    /// 编译目标：`wasm` / `jvm` / `clr`（verify / lir 模式）。
    pub target_platform: Option<String>,
    /// 是否以 JSON 格式输出。
    pub json: Option<bool>,
    /// 是否 dump 函数体原始字节（wasm）。
    pub hex: Option<bool>,
    /// 是否结构化解析 Type 段（wasm）。
    pub types: Option<bool>,
    /// 是否审计 wasm-gc struct/array 类型覆盖。
    pub gc_audit: Option<bool>,
    /// 是否审计 Node JS-glue 契约。
    pub glue_audit: Option<bool>,
}

pub use host_api::{legion_build, legion_plan};

/// 运行 Legion CLI（stdout/stderr 写入 Node 进程终端，不捕获）。
///
/// 新宿主请优先使用 [`legion_plan`] / [`legion_build`]；本函数仅保留给 wasm collect 与旧脚本。
#[napi]
pub fn legion_run(argv: Vec<String>) -> i32 {
    cli::run_with_argv(argv.into_iter().map(OsString::from).collect())
}

/// 运行 Legion CLI 并捕获 stdout/stderr。
///
/// 新宿主请优先使用 [`legion_plan`] / [`legion_build`]；本函数仅保留给 wasm collect 回退。
#[napi]
pub fn legion_run_captured(argv: Vec<String>) -> LegionRunOutcome {
    let os_argv: Vec<OsString> = argv.into_iter().map(OsString::from).collect();
    let (status, stdout, stderr) = thread::Builder::new()
        .name("legion-captured".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || stdio_capture::capture(|| cli::run_with_argv_in_current_thread(os_argv)))
        .expect("failed to spawn legion captured thread")
        .join()
        .expect("legion captured thread panicked");
    LegionRunOutcome { status: status as u32, stdout, stderr }
}

/// 运行 `legion spy <mode>` 并捕获输出。
#[napi]
pub fn legion_spy_run(mode: String, options: SpyTargetRunOptions) -> napi::Result<LegionRunOutcome> {
    let spy_opts = map_spy_options(&mode, options)?;
    let (status, stdout, stderr) = thread::Builder::new()
        .name("legion-spy".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            stdio_capture::capture(|| match run_spy(&spy_opts) {
                Ok(code) => exit_code_to_i32(code),
                Err(report) => {
                    eprintln!("{report:?}");
                    1
                }
            })
        })
        .expect("failed to spawn legion spy thread")
        .join()
        .expect("legion spy thread panicked");
    Ok(LegionRunOutcome { status: status as u32, stdout, stderr })
}

/// 解析 argv 但不执行（仅供 wasm collect / 旧脚本；新宿主请用 `@vmz/commander` + [`legion_plan`] / [`legion_build`]）。
#[napi]
pub fn legion_parse(argv: Vec<String>) -> napi::Result<String> {
    use clap::Parser;
    use legion::cli::LegionCli;

    let os_argv: Vec<OsString> = argv.into_iter().map(OsString::from).collect();
    if legion::forward::try_forward_companion(&os_argv).is_some() {
        return Ok("forward".to_string());
    }
    let cli = LegionCli::try_parse_from(&os_argv).map_err(|error| napi::Error::from_reason(error.to_string()))?;
    Ok(format!("{cli:?}"))
}

fn exit_code_to_i32(code: ExitCode) -> i32 {
    if code == ExitCode::SUCCESS { 0 } else { 1 }
}

fn map_spy_options(mode: &str, options: SpyTargetRunOptions) -> napi::Result<SpyOptions> {
    let target = SpyTargetOptions {
        input: options.input,
        func: options.func,
        method: options.method,
        offset: options.offset,
        list: options.list.unwrap_or(false),
        context: options.context.unwrap_or(20) as usize,
        target_platform: options.target_platform.as_deref().map(parse_spy_target_platform).transpose()?,
        json: options.json.unwrap_or(false),
        hex: options.hex.unwrap_or(false),
        types: options.types.unwrap_or(false),
        gc_audit: options.gc_audit.unwrap_or(false),
        glue_audit: options.glue_audit.unwrap_or(false),
    };

    let mode = match mode.trim().to_ascii_lowercase().as_str() {
        "wasm" => SpyMode::Wasm(target),
        "jvm" => SpyMode::Jvm(target),
        "clr" => SpyMode::Clr(target),
        "native" => SpyMode::Native(target),
        "lir" => SpyMode::Lir(target),
        "mir" => SpyMode::Mir(target),
        "verify" => SpyMode::Verify(target),
        other => {
            return Err(napi::Error::from_reason(format!("unsupported spy mode '{other}' (expected wasm/jvm/clr/native/lir/mir/verify)")));
        }
    };

    Ok(SpyOptions { mode })
}

fn parse_spy_target_platform(value: &str) -> napi::Result<SpyTargetPlatform> {
    match value.trim().to_ascii_lowercase().as_str() {
        "wasm" => Ok(SpyTargetPlatform::Wasm),
        "jvm" => Ok(SpyTargetPlatform::Jvm),
        "clr" => Ok(SpyTargetPlatform::Clr),
        other => Err(napi::Error::from_reason(format!("unsupported spy target platform '{other}' (expected wasm/jvm/clr)"))),
    }
}
