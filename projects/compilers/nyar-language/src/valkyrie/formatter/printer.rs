//! Printer 语言插件（平台契约见 `nyar_analyzer::format`）。
//!
//! **数据模型** → 文本（序列化 / 调试）。**不是**源码正规格式化；
//! 不保证注释与空白保留。源码 fmt 请用 [`crate::formatter::format_source`]。

use std::{any::Any, sync::OnceLock};

use std_data::text::{msil::MsilModule, von::VonValue, wat::WatDocument, wit::WitPackage};

use super::{FormatError, FormatOptions, PrintStyle, Printer, PrinterRegistry};
use crate::text::{
    msil::MsilTextWriter,
    von::{format_von_compact, format_von_pretty},
    wat::format_wat_document,
    wit::format_wit_package,
};

struct VonPrinter;

impl super::Printer for VonPrinter {
    fn language_id(&self) -> &str {
        "von"
    }

    fn print(&self, document: &dyn Any, style: PrintStyle, options: &FormatOptions) -> Result<String, FormatError> {
        let value = document.downcast_ref::<VonValue>().ok_or_else(|| FormatError::WrongDocument { expected: "VonValue".into() })?;
        let out = match style {
            PrintStyle::Compact => format_von_compact(value),
            PrintStyle::Indented => format_von_pretty(value, options.indent_width),
        };
        Ok(out)
    }
}

struct WatPrinter;
impl super::Printer for WatPrinter {
    fn language_id(&self) -> &str {
        "wat"
    }

    fn print(&self, document: &dyn Any, _style: PrintStyle, options: &FormatOptions) -> Result<String, FormatError> {
        let document = document.downcast_ref::<WatDocument>().ok_or_else(|| FormatError::WrongDocument { expected: "WatDocument".into() })?;
        Ok(format_wat_document(document))
    }
}

struct WitPrinter;
impl super::Printer for WitPrinter {
    fn language_id(&self) -> &str {
        "wit"
    }

    fn print(&self, document: &dyn Any, _style: PrintStyle, options: &FormatOptions) -> Result<String, FormatError> {
        let package = document.downcast_ref::<WitPackage>().ok_or_else(|| FormatError::WrongDocument { expected: "WitPackage".into() })?;
        Ok(format_wit_package(package))
    }
}

struct MsilPrinter;
impl super::Printer for MsilPrinter {
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

/// 打印 `VonValue`。
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

/// 通过 serde 将 `value` 序列化为紧凑 VON 文本（走 printer 注册表）。
#[cfg(feature = "serde")]
pub fn to_string<T>(value: &T) -> Result<String, std_data::text::von::VonError>
where
    T: serde::Serialize,
{
    let von_value = std_data::text::von::to_value(value)?;
    Ok(print_von(&von_value, PrintStyle::Compact, &FormatOptions::default()).expect("VonValue compact print is infallible"))
}

/// 经 printer 引擎将 `value` 序列化为缩进 VON 文本。
#[cfg(feature = "serde")]
pub fn to_string_indented<T>(value: &T) -> Result<String, std_data::text::von::VonError>
where
    T: serde::Serialize,
{
    let von_value = std_data::text::von::to_value(value)?;
    Ok(print_von(&von_value, PrintStyle::Indented, &FormatOptions::default()).expect("VonValue indented print is infallible"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn von_print_via_registry() {
        let mut fields = BTreeMap::new();
        fields.insert("x".into(), VonValue::Number(1));
        let value = VonValue::Object(fields);
        let out = print_von(&value, PrintStyle::Compact, &FormatOptions::default()).unwrap();
        assert_eq!(out, "{x: 1}");
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
