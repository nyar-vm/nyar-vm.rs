#![doc = include_str!("readme.md")]

#[cfg(feature = "format-spy")]
pub mod clr;
#[cfg(feature = "format-spy")]
pub mod jvm;
#[cfg(feature = "format-spy")]
pub mod native;
#[cfg(feature = "format-spy")]
pub mod pe_dump;
#[cfg(feature = "format-spy")]
pub mod wasm;

use clap::{Args, Subcommand, ValueEnum};
use miette::{Result, miette};
use std::process::ExitCode;

/// spy 子命令的选项集合。
#[derive(Debug, Clone, Args)]
pub struct SpyOptions {
    /// 诊断模式：`wasm` / `jvm` / `clr` / `native` / `lir` / `mir` / `verify`。
    #[command(subcommand)]
    pub mode: SpyMode,
}

/// spy 的模式枚举。
#[derive(Debug, Clone, Subcommand)]
pub enum SpyMode {
    /// 反汇编 WASM 二进制。
    Wasm(SpyTargetOptions),
    /// 反汇编 JVM 字节码。
    Jvm(SpyTargetOptions),
    /// dump CLR / MSIL。
    Clr(SpyTargetOptions),
    /// 反汇编 Native x86-64 PE/ELF 或 X64Instruction JSON dump。
    Native(SpyTargetOptions),
    /// dump 指定函数的 LIR。
    Lir(SpyTargetOptions),
    /// dump 指定函数的 MIR。
    Mir(SpyTargetOptions),
    /// 构建、验证并自动定位错误。
    Verify(SpyTargetOptions),
}

/// 各个 spy 模式共享的参数。
#[derive(Debug, Clone, Args)]
pub struct SpyTargetOptions {
    /// 目标文件路径（wasm/jvm/clr 模式）或项目名（lir/mir/verify 模式）。
    pub input: Option<String>,
    /// 函数索引或函数名（wasm/lir/mir 模式）；JVM jar 模式下过滤 class 内部名/二进制名片段。
    #[arg(long, short = 'f')]
    pub func: Option<String>,
    /// 方法名（jvm/clr 模式）。
    #[arg(long, short = 'm')]
    pub method: Option<String>,
    /// 绝对偏移量（wasm 模式，用于定位验证错误）。
    #[arg(long, short = 'o')]
    pub offset: Option<i64>,
    /// 是否列出所有函数/方法。
    #[arg(long, short = 'l')]
    pub list: bool,
    /// 错误点上下文行数（默认 20）。
    #[arg(long, short = 'c', default_value_t = 20)]
    pub context: usize,
    /// 编译目标（verify / lir 模式）：`wasm` / `jvm` / `clr`。
    #[arg(long = "target", short = 't', value_enum)]
    pub target_platform: Option<SpyTargetPlatform>,
    /// 是否以 JSON 格式输出。
    #[arg(long)]
    pub json: bool,
    /// 是否 dump 函数体原始字节（wasm 模式，配合 `--func` 使用）。
    #[arg(long)]
    pub hex: bool,
    /// 是否结构化解析 Type 段（wasm 模式，列出每个 type 条目的索引/种类/内容）。
    #[arg(long)]
    pub types: bool,
    /// 审计 wasm-gc struct/array 类型覆盖（wasm 模式，读取 legion.wasm.gc_layouts）。
    #[arg(long = "gc-audit")]
    pub gc_audit: bool,
    /// 审计 Node JS-glue 契约：`cli_get_*` 导入与 `help`/`version`/`build`/`main` 导出是否匹配。
    #[arg(long = "glue-audit")]
    pub glue_audit: bool,
}

/// 可选的编译目标。
#[derive(Debug, Clone, PartialEq, Eq, ValueEnum)]
pub enum SpyTargetPlatform {
    /// WASM 二进制目标。
    Wasm,
    /// JVM class/JAR 目标。
    Jvm,
    /// CLR / MSIL 目标。
    Clr,
}

impl SpyOptions {
    /// 返回模式名与对应参数。
    pub fn split(&self) -> (&'static str, &SpyTargetOptions) {
        match &self.mode {
            SpyMode::Wasm(options) => ("wasm", options),
            SpyMode::Jvm(options) => ("jvm", options),
            SpyMode::Clr(options) => ("clr", options),
            SpyMode::Native(options) => ("native", options),
            SpyMode::Lir(options) => ("lir", options),
            SpyMode::Mir(options) => ("mir", options),
            SpyMode::Verify(options) => ("verify", options),
        }
    }
}

/// 执行 spy 子命令。
///
/// 根据模式分发到对应的诊断子模块，未指定模式时打印帮助信息。
pub fn run(options: &SpyOptions) -> Result<ExitCode> {
    #[cfg(not(feature = "format-spy"))]
    {
        let (mode, _) = options.split();
        return Err(miette!("legion spy `{mode}` 不属于正式 build/run 链；请使用 `cargo build -p legion --features format-spy` 装配诊断能力"));
    }
    #[cfg(feature = "format-spy")]
    {
        match &options.mode {
            SpyMode::Clr(_) => clr::run(options),
            SpyMode::Wasm(_) => wasm::run(options),
            SpyMode::Jvm(_) => jvm::run(options),
            SpyMode::Native(_) => native::run(options),
            SpyMode::Lir(_) | SpyMode::Mir(_) | SpyMode::Verify(_) => {
                let (mode, _) = options.split();
                Err(miette!("spy 模式 '{}' 尚未实现，当前只支持 `clr`、`wasm`、`jvm` 和 `native`", mode))
            }
        }
    }
}
