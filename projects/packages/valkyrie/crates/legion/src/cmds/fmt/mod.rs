//! `legion fmt`：格式化 `.v` / `.vx` / `.von` / `.awsl`。

use std::process::ExitCode;

use clap::Args;
use miette::{Result, miette};
use nyar_language::formatter::{FormatCliOptions, format_options_from_cli, normalize_extensions, report_format_cli_with, run_format_cli_with};

/// `legion fmt` 参数。
#[derive(Debug, Clone, Args)]
pub struct FmtArgs {
    /// 要格式化的文件或目录（默认当前目录）。
    #[arg(value_name = "PATH")]
    pub paths: Vec<std::path::PathBuf>,
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
}

impl FmtArgs {
    fn to_cli_options(&self) -> FormatCliOptions {
        let extensions = if self.extensions.is_empty() { None } else { Some(normalize_extensions(&self.extensions)) };
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

/// 执行 `legion fmt`。
pub fn run(args: &FmtArgs) -> Result<ExitCode> {
    let options = args.to_cli_options();
    let report = run_format_cli_with(&options).map_err(|error| miette!("{error}"))?;
    if report_format_cli_with(&report, &options) { Ok(ExitCode::FAILURE) } else { Ok(ExitCode::SUCCESS) }
}
