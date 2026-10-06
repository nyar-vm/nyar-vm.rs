//! AST **pretty printer** 注册表（平台契约见 `nyar_analyzer::format`）。
//!
//! 与 [`crate::formatter`]（CST 源码正规格式化）是两套完全不同的概念，对齐 Oak 各语言 crate 的
//! `printer/` 与 `formatter/` 并列目录（见 `oak-typescript`）。
//!
//! - **Printer**：已解析 AST / 值模型 → 文本；不保留注释与空白。
//! - **Formatter**：源码 → CST token-gap → 文本；保留 trivia。
//!
//! VON 打印当前委托 `oak-von` AST `ToSource`；待 `oak-von::printer` 落地后改为一行转发。

use std::{any::Any, sync::OnceLock};

use nyar_analyzer::format::{FormatError, FormatOptions, PrintStyle, Printer, PrinterRegistry};
use oak_core::source::{SourceBuffer, ToSource};
use oak_von::{VonValue, language::value::to_ast};
use std_data::text::msil::MsilModule;

use crate::{wat::WatDocument, wit::WitPackage};
use crate::text::{msil::MsilTextWriter, wat::format_wat_document, wit::format_wit_package};

fn print_von_value(value: &VonValue, _style: PrintStyle, _options: &FormatOptions) -> String {
    let ast = to_ast(value);
    let mut buffer = SourceBuffer::new();
    ast.to_source(&mut buffer);
    buffer.to_string()
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

/// 经 Oak VON AST printer 将 `value` 写出为文本。
///
/// 缩进样式仍待 `oak-von::printer` 落地；当前与 [`to_string`] 同样输出紧凑文本。
#[cfg(feature = "serde")]
pub fn to_string_indented<T>(value: &T) -> Result<String, oak_core::OakError>
where
    T: serde::Serialize,
{
    oak_von::to_string(value)
}

#[cfg(test)]
mod tests {
    use oak_von::language::value::{VonField, VonObject};

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
    fn wrong_document_type() {
        let value = VonValue::Null;
        let err = print_document("wat", &value, PrintStyle::Indented, &FormatOptions::default()).unwrap_err();
        assert!(matches!(err, FormatError::WrongDocument { .. }));
    }
}
