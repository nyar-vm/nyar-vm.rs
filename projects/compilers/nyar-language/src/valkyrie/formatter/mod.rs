//! 语言侧源码格式化实现（平台契约见 `nyar_analyzer::format`）。
//!
//! - **SourceFormatter**：源码 → 正规格式化（CST，保留 trivia）
//! - **Printer**：数据模型 → 文本（序列化，不保证 trivia）
//! - **配置**：默认从各文件路径的 `.editorconfig` 解析（[`FormatBatchOptions::use_editorconfig`]）

mod batch;
mod cli;
mod printer;
mod surface;

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub use crate::text::{FormatSyntax, ToDocument};
pub use batch::{FormatBatchOptions, FormatBatchReport, format_paths};
pub use cli::{
    FormatCliOptions, format_options_from_cli, normalize_extensions, report_format_cli, report_format_cli_with, run_format_cli,
    run_format_cli_with,
};
pub use nyar_analyzer::format::{
    ByteRange, Document, FormatConfigLoader, FormatConfigResolution, FormatError, FormatOptions, FormattedOutput, PrintStyle, Printer,
    PrinterProvider, PrinterRegistry, SourceFormatter, SourceFormatterProvider, SourceFormatterRegistry, SourceMap,
};
pub use printer::{print_document, print_msil_module, print_von, print_wat, print_wit_package, printer_registry};
#[cfg(feature = "serde")]
pub use printer::{to_string, to_string_indented};
pub use surface::{
    AWSL_EXTENSIONS, V_EXTENSIONS, VON_EXTENSIONS, VX_EXTENSIONS, is_valkyrie_family_extension, source_kind_from_extension,
    valkyrie_family_extensions,
};

#[deprecated(note = "use CST + Document via SourceFormatter")]
pub use nyar_analyzer::format::FormatBuffer;

/// 可格式化的源码种类（语言侧便利枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// 核心 Valkyrie：`.v` / `.valkyrie`（std-data `text::valkyrie`，无 X-Grammar）。
    V,
    /// Valkyrie + X-Grammar：`.vx`（widget / markup，仍走 valkyrie CST，独立 `.editorconfig` 段）。
    Vx,
    /// VON 数据：`.von`（std-data `text::von`）。
    Von,
    /// Asgard AWSL 模板：`.awsl`（std-data `text::awsl`，与 V/Vx 不同引擎）。
    Awsl,
}

impl SourceKind {
    /// 对应 `SourceFormatter::language_id`。
    pub fn language_id(self) -> &'static str {
        match self {
            Self::V => "v",
            Self::Vx => "vx",
            Self::Von => "von",
            Self::Awsl => "awsl",
        }
    }

    /// 由文件扩展名推断种类。
    pub fn from_extension(ext: &str) -> Option<Self> {
        surface::source_kind_from_extension(ext)
    }

    /// 由路径推断。
    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension().and_then(|ext| ext.to_str()).and_then(Self::from_extension)
    }
}

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
        crate::valkyrie::source_format::format_valkyrie(source, options, self.vx)
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
        reg.register(&["javascript", "js", "jsx", "mjs", "cjs"], Box::new(crate::javascript::JavascriptFormatter::new("javascript")));
        reg.register(&["typescript", "ts", "tsx", "mts", "cts"], Box::new(crate::javascript::JavascriptFormatter::new("typescript")));
        reg.register(&["json"], Box::new(crate::javascript::JavascriptFormatter::new("json")));
        reg.register(&["python", "py"], Box::new(crate::python::PythonFormatter));
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
#[cfg(test)]
mod tests {
    use super::*;

    fn assert_idempotent(kind: SourceKind, source: &str) {
        let options = FormatOptions::default();
        let once = format_source(kind, source, &options).expect("format once");
        let twice = format_source(kind, &once, &options).expect("format twice");
        assert_eq!(once, twice, "format should be idempotent");
    }

    #[test]
    fn source_kind_from_extension() {
        assert_eq!(SourceKind::from_extension("v"), Some(SourceKind::V));
        assert_eq!(SourceKind::from_extension("valkyrie"), Some(SourceKind::V));
        assert_eq!(SourceKind::from_extension("vx"), Some(SourceKind::Vx));
        assert_eq!(SourceKind::from_extension("von"), Some(SourceKind::Von));
        assert_eq!(SourceKind::from_extension("awsl"), Some(SourceKind::Awsl));
        assert_eq!(SourceKind::from_extension("vk"), None);
        assert_eq!(SourceKind::from_extension("rs"), None);
    }

    #[test]
    fn format_v_idempotent() {
        assert_idempotent(SourceKind::V, "micro main(){let x=1}");
    }

    #[test]
    fn format_awsl_idempotent() {
        let source = r#"
<template>
<div class="box"><span>{title}</span></div>
</template>
"#;
        assert_idempotent(SourceKind::Awsl, source);
    }

    #[test]
    fn format_von_idempotent_with_comment() {
        assert_idempotent(SourceKind::Von, "# note\n{ x: 1 }\n");
    }

    #[test]
    fn format_v_idempotent_with_comment() {
        assert_idempotent(SourceKind::V, "# note\nmicro main(){let x=1}\n");
    }

    #[test]
    fn format_von_idempotent() {
        assert_idempotent(SourceKind::Von, r#"{ "x": 1, "y": [true, null] }"#);
    }

    #[test]
    fn format_vx_idempotent() {
        assert_idempotent(SourceKind::Vx, "namespace demo;");
    }

    #[test]
    fn format_paths_check_detects_dirty() {
        let dir = std::env::temp_dir().join(format!("nyar_fmt_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.von");
        std::fs::write(&path, r#"{ "a":1 }"#).unwrap();

        let check = FormatBatchOptions { check: true, ..FormatBatchOptions::default() };
        let report = format_paths(&[dir.clone()], &check).expect("check");
        assert_eq!(report.checked, 1);
        assert_eq!(report.changed, 1);

        let write = FormatBatchOptions { check: false, ..FormatBatchOptions::default() };
        let report = format_paths(&[dir.clone()], &write).expect("write");
        assert_eq!(report.changed, 1);

        let report = format_paths(&[dir.clone()], &check).expect("recheck");
        assert_eq!(report.changed, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
