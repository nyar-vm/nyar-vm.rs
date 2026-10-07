//! AST **pretty printer** 注册表（平台契约见 `nyar_analyzer::format`）。
//!
//! 与 [`crate::formatter`]（CST 源码正规格式化）是两套完全不同的概念，对齐 Oak 各语言 crate 的
//! `printer/` 与 `formatter/` 并列目录（见 `oak-typescript`）。
//!
//! - **Printer**：已解析 AST / 值模型 → `oak_pretty_print::Document` → 文本；不保留注释与空白。
//! - **Formatter**：源码 → CST token-gap → 文本；保留 trivia。
//!
//! Valkyrie / VON 的 AST print 均委托对应 `oak-*::printer`；不得在此手写布局规则。

use std::{any::Any, sync::OnceLock};

use nyar_analyzer::format::{FormatError, FormatOptions, PrintStyle, Printer, PrinterRegistry};
use oak_valkyrie::printer::{
    Document as PrettyDocument, PrintOptions as ValkyriePrintOptions, PrintStyle as ValkyriePrintStyle,
    print_root as oak_print_valkyrie_root, print_source as oak_print_valkyrie_source, render_document as oak_render_valkyrie_document,
    to_document as oak_valkyrie_to_document,
};
use oak_von::printer::{PrintOptions as VonPrintOptions, PrintStyle as VonPrintStyle, print_value};
use oak_von::VonValue;

use crate::transitional::msil::MsilModule;
use crate::valkyrie::frontend::ValkyrieRoot;
use crate::valkyrie::text::ToDocument;
use crate::{wat::WatDocument, wit::WitPackage};
use crate::text::{msil::MsilTextWriter, wat::format_wat_document, wit::format_wit_package};

/// Oak pretty-print 布局文档（AST printer 路径；非 `nyar_analyzer::format::Document`）。
pub type Document = PrettyDocument<'static>;

fn map_von_print_style(style: PrintStyle) -> VonPrintStyle {
    match style {
        PrintStyle::Compact => VonPrintStyle::Compact,
        PrintStyle::Indented => VonPrintStyle::Indented,
    }
}

fn map_von_print_options(style: PrintStyle, options: &FormatOptions) -> VonPrintOptions {
    VonPrintOptions { style: map_von_print_style(style), indent_width: options.indent_width, max_width: options.max_width }
}

fn map_valkyrie_print_style(style: PrintStyle) -> ValkyriePrintStyle {
    match style {
        PrintStyle::Compact => ValkyriePrintStyle::Compact,
        PrintStyle::Indented => ValkyriePrintStyle::Indented,
    }
}

fn map_valkyrie_print_options(style: PrintStyle, options: &FormatOptions) -> ValkyriePrintOptions {
    ValkyriePrintOptions {
        style: map_valkyrie_print_style(style),
        indent_width: options.indent_width,
        max_width: options.max_width,
    }
}

fn map_valkyrie_print_error(error: oak_valkyrie::printer::PrintError) -> FormatError {
    match error {
        oak_valkyrie::printer::PrintError::Parse(message) => FormatError::Parse { path: None, message },
        oak_valkyrie::printer::PrintError::Unsupported { context } => FormatError::Parse { path: None, message: context },
    }
}

fn print_von_value(value: &VonValue, style: PrintStyle, options: &FormatOptions) -> String {
    print_value(value, &map_von_print_options(style, options))
}

/// 将 `ValkyrieRoot` 转为 `oak-pretty-print` 布局文档。
pub fn to_document(root: &ValkyrieRoot) -> Result<Document, FormatError> {
    oak_valkyrie_to_document(root).map_err(map_valkyrie_print_error)
}

/// 渲染 Valkyrie pretty-print 文档为文本。
pub fn render_document(doc: &Document, style: PrintStyle, options: &FormatOptions) -> String {
    oak_render_valkyrie_document(doc, &map_valkyrie_print_options(style, options))
}

/// 将 `ValkyrieRoot` 写出为文本（AST print；非 CST formatter）。
pub fn print_valkyrie_root(root: &ValkyrieRoot, style: PrintStyle, options: &FormatOptions) -> Result<String, FormatError> {
    oak_print_valkyrie_root(root, &map_valkyrie_print_options(style, options)).map_err(map_valkyrie_print_error)
}

/// Valkyrie 源码 AST print（parse → `Document` → text；非 CST formatter）。
pub fn print_valkyrie_source(source: &str, style: PrintStyle, options: &FormatOptions) -> Result<String, FormatError> {
    oak_print_valkyrie_source(source, &map_valkyrie_print_options(style, options)).map_err(map_valkyrie_print_error)
}

struct ValkyriePrinter;

impl Printer for ValkyriePrinter {
    fn language_id(&self) -> &str {
        "v"
    }

    fn print(&self, document: &dyn Any, style: PrintStyle, options: &FormatOptions) -> Result<String, FormatError> {
        let root = document.downcast_ref::<ValkyrieRoot>().ok_or_else(|| FormatError::WrongDocument { expected: "ValkyrieRoot".into() })?;
        print_valkyrie_root(root, style, options)
    }
}

struct VonPrinter;

impl Printer for VonPrinter {
    fn language_id(&self) -> &str {
        "von"
    }

    fn print(&self, document: &dyn Any, style: PrintStyle, options: &FormatOptions) -> Result<String, FormatError> {
        let value = document.downcast_ref::<VonValue>().ok_or_else(|| FormatError::WrongDocument { expected: "VonValue".into() })?;
        Ok(print_von_value(value, style, options))
    }
}

struct WatPrinter;
impl Printer for WatPrinter {
    fn language_id(&self) -> &str {
        "wat"
    }

    fn print(&self, document: &dyn Any, _style: PrintStyle, _options: &FormatOptions) -> Result<String, FormatError> {
        let document = document.downcast_ref::<WatDocument>().ok_or_else(|| FormatError::WrongDocument { expected: "WatDocument".into() })?;
        Ok(format_wat_document(document))
    }
}

struct WitPrinter;
impl Printer for WitPrinter {
    fn language_id(&self) -> &str {
        "wit"
    }

    fn print(&self, document: &dyn Any, _style: PrintStyle, _options: &FormatOptions) -> Result<String, FormatError> {
        let package = document.downcast_ref::<WitPackage>().ok_or_else(|| FormatError::WrongDocument { expected: "WitPackage".into() })?;
        Ok(format_wit_package(package))
    }
}

struct MsilPrinter;
impl Printer for MsilPrinter {
    fn language_id(&self) -> &str {
        "msil"
    }

    fn print(&self, document: &dyn Any, _style: PrintStyle, options: &FormatOptions) -> Result<String, FormatError> {
        let module = document.downcast_ref::<MsilModule>().ok_or_else(|| FormatError::WrongDocument { expected: "MsilModule".into() })?;
        let mut writer = MsilTextWriter::new().with_indent_text(" ".repeat(options.indent_width));
        Ok(writer.write_module(module))
    }
}

/// 语言侧 printer 注册表。
pub fn printer_registry() -> &'static PrinterRegistry {
    static REGISTRY: OnceLock<PrinterRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut reg = PrinterRegistry::new();
        reg.register(&["v", "vx", "valkyrie"], Box::new(ValkyriePrinter));
        reg.register(&["von"], Box::new(VonPrinter));
        reg.register(&["wat"], Box::new(WatPrinter));
        reg.register(&["wit"], Box::new(WitPrinter));
        reg.register(&["msil", "il"], Box::new(MsilPrinter));
        reg
    })
}

/// 按语言 id 将已解析文档写出为文本。
pub fn print_document(language_id: &str, document: &dyn Any, style: PrintStyle, options: &FormatOptions) -> Result<String, FormatError> {
    printer_registry().print(language_id, document, style, options)
}

/// 打印 `ValkyrieRoot`（AST pretty print，非 CST formatter）。
pub fn print_valkyrie(root: &ValkyrieRoot, style: PrintStyle, options: &FormatOptions) -> Result<String, FormatError> {
    print_document("v", root, style, options)
}

/// 打印 `VonValue`（AST pretty print，非 CST formatter）。
pub fn print_von(value: &VonValue, style: PrintStyle, options: &FormatOptions) -> Result<String, FormatError> {
    print_document("von", value, style, options)
}

/// 打印 WAT 文档。
pub fn print_wat(document: &WatDocument, options: &FormatOptions) -> Result<String, FormatError> {
    print_document("wat", document, PrintStyle::Indented, options)
}

/// 打印 WIT package。
pub fn print_wit_package(package: &WitPackage, options: &FormatOptions) -> Result<String, FormatError> {
    print_document("wit", package, PrintStyle::Indented, options)
}

/// 打印 MSIL 模块（默认整模块文本；高级场景请用 `MsilTextWriter`）。
pub fn print_msil_module(module: &MsilModule, options: &FormatOptions) -> Result<String, FormatError> {
    print_document("msil", module, PrintStyle::Indented, options)
}

/// 通过 serde 将 `value` 序列化为紧凑 VON 文本（AST print 路径）。
#[cfg(feature = "serde")]
pub fn to_string<T>(value: &T) -> Result<String, oak_core::OakError>
where
    T: serde::Serialize,
{
    oak_von::to_string(value)
}

/// 经 `oak-von::printer` 将 `value` 写出为缩进 VON 文本。
#[cfg(feature = "serde")]
pub fn to_string_indented<T>(value: &T) -> Result<String, oak_core::OakError>
where
    T: serde::Serialize,
{
    oak_von::to_string_indented(value, FormatOptions::default().indent_width)
}

#[cfg(test)]
mod tests {
    use oak_von::language::value::{VonField, VonObject};

    use crate::valkyrie::frontend::parse_source;

    use super::*;

    #[test]
    fn von_print_via_registry() {
        let value = VonValue::Object(VonObject { fields: vec![VonField { name: "x".into(), value: VonValue::Number(1.0) }] });
        let out = print_von(&value, PrintStyle::Compact, &FormatOptions::default()).unwrap();
        assert_eq!(out, "{x=1}");
        let pretty = print_von(&value, PrintStyle::Indented, &FormatOptions::default()).unwrap();
        assert!(pretty.contains('x'));
    }

    #[test]
    fn valkyrie_source_print_via_oak() {
        let out = print_valkyrie_source("micro main(){let x=1}", PrintStyle::Compact, &FormatOptions::default()).unwrap();
        assert!(out.contains("micro main()"));
        assert!(out.contains("let x=1"));
    }

    #[test]
    fn valkyrie_ast_print_produces_document() {
        let root = parse_source("micro main(){let x=1}").expect("parse");
        let doc = to_document(&root).expect("to_document");
        let compact = render_document(&doc, PrintStyle::Compact, &FormatOptions::default());
        assert!(compact.contains("micro main()"));
        let via_trait = root.to_document().expect("trait to_document");
        let again = render_document(&via_trait, PrintStyle::Compact, &FormatOptions::default());
        assert_eq!(compact, again);
        let via_registry = print_valkyrie(&root, PrintStyle::Compact, &FormatOptions::default()).unwrap();
        assert_eq!(compact, via_registry);
    }

    #[test]
    fn wrong_document_type() {
        let value = VonValue::Null;
        let err = print_document("wat", &value, PrintStyle::Indented, &FormatOptions::default()).unwrap_err();
        assert!(matches!(err, FormatError::WrongDocument { .. }));
    }
}
