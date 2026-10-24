//! LSP / CLI **格式化注册表**（契约见 `nyar_analyzer::format::SourceFormatterRegistry`）。
//!
//! **实现不在此 crate**：合法 formatter 只能由上游 `oak-<language>/src/formatter/` 提供
//!（见 `oak-typescript::formatter`）。Valkyrie / VON / AWSL 已转发至对应 Oak formatter；
//! 不得在此新增 CST token-gap 排版规则。
//!
//! 与 [`crate::printer`]（AST pretty print）严格分离，对齐 Oak 的 `formatter/` vs `printer/`；
//! 早期把 printer API 与 deprecated `FormatBuffer` 从本模块重导出的混合接口已删除，
//! 消费者必须直接使用 `crate::printer`。
//!
//! - **SourceFormatter**：源码 → CST token-gap → 文本（保留 trivia）
//! - **配置**：默认从各文件路径的 `.editorconfig` 解析（[`FormatBatchOptions::use_editorconfig`]）

mod batch;
mod cli;

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub use batch::{FormatBatchOptions, FormatBatchReport, format_paths};
pub use cli::{
    FormatCliOptions, format_options_from_cli, normalize_extensions, report_format_cli, report_format_cli_with, run_format_cli,
    run_format_cli_with,
};
pub use nyar_analyzer::format::{
    ByteRange, Document, FormatConfigLoader, FormatConfigResolution, FormatError, FormatOptions, FormattedOutput, PrintStyle, Printer,
    PrinterProvider, PrinterRegistry, SourceFormatter, SourceFormatterProvider, SourceFormatterRegistry, SourceMap,
};
pub use rusty_valkyrie::{
    source_surface::{
        AWSL_EXTENSIONS, SourceKind, V_EXTENSIONS, VON_EXTENSIONS, VX_EXTENSIONS, is_valkyrie_family_extension, source_kind_from_extension,
        valkyrie_family_extensions,
    },
    text::{FormatSyntax, ToDocument},
};

/// 单文件格式化结果（不写盘）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatFileOutcome {
    /// 源文件路径。
    pub path: PathBuf,
    /// 格式化后的文本。
    pub formatted: String,
    /// 是否与原始内容不同。
    pub changed: bool,
}

struct ValkyrieFormatter {
    vx: bool,
}

impl SourceFormatter for ValkyrieFormatter {
    fn language_id(&self) -> &str {
        if self.vx { "vx" } else { "v" }
    }

    fn format_document(&self, source: &str, options: &FormatOptions) -> Result<FormattedOutput, FormatError> {
        rusty_valkyrie::source_format::format_valkyrie(source, options, self.vx)
    }
}

struct VonSourceFormatter;
impl SourceFormatter for VonSourceFormatter {
    fn language_id(&self) -> &str {
        "von"
    }

    fn format_document(&self, source: &str, options: &FormatOptions) -> Result<FormattedOutput, FormatError> {
        crate::von::source_format::format_von_source(source, options)
    }
}

struct AwslSourceFormatter;
impl SourceFormatter for AwslSourceFormatter {
    fn language_id(&self) -> &str {
        "awsl"
    }

    fn format_document(&self, source: &str, options: &FormatOptions) -> Result<FormattedOutput, FormatError> {
        crate::awsl::source_format::format_awsl(source, options)
    }
}

fn source_registry() -> &'static SourceFormatterRegistry {
    static REGISTRY: OnceLock<SourceFormatterRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut reg = SourceFormatterRegistry::new();
        reg.register(&["v"], Box::new(ValkyrieFormatter { vx: false }));
        reg.register(&["vx"], Box::new(ValkyrieFormatter { vx: true }));
        reg.register(&["von"], Box::new(VonSourceFormatter));
        reg.register(&["awsl"], Box::new(AwslSourceFormatter));
        // JS/TS/JSON 与 Python 的源码排版归各自 Oak formatter（`oak-typescript::formatter` 等）；
        // 本 crate 不得再注册「只裁剪空白行」的第二套本地 formatter 冒充格式化。
        // 在这些 Oak formatter 接入前，相应扩展名按 unsupported 失败。
        reg
    })
}

/// 将源码格式化为规范文本。
pub fn format_source(kind: SourceKind, source: &str, options: &FormatOptions) -> Result<String, FormatError> {
    source_registry().format(kind.language_id(), source, options)
}

/// 格式化选中字节范围。
pub fn format_source_range(kind: SourceKind, source: &str, range: ByteRange, options: &FormatOptions) -> Result<FormattedOutput, FormatError> {
    source_registry().format_range(kind.language_id(), source, range, options)
}

/// 读取路径并格式化（不写盘）。
pub fn format_path(path: &Path, options: &FormatOptions) -> Result<FormatFileOutcome, FormatError> {
    let kind = SourceKind::from_path(path).ok_or_else(|| FormatError::unsupported_extension(path.to_path_buf()))?;
    let source = std::fs::read_to_string(path).map_err(|error| FormatError::Io { path: path.to_path_buf(), source: error })?;
    let formatted = format_source(kind, &source, options)?;
    Ok(FormatFileOutcome { path: path.to_path_buf(), changed: formatted != source, formatted })
}
