//! 过渡：`vcc-data` 值模型的紧凑/缩进文本，仅供 CST formatter 在 Oak `formatter` 落地前使用。
//!
//! 源码正规格式化属于 Oak 上游（对齐 `oak-typescript::formatter`），不在此扩展规则。
//! 模型 printer 与 serde 走 `oak-von::ToSource` / [`oak_von::to_string`]，见 [`crate::formatter::printer`]。

use std_data::text::von::VonValue;

pub fn format_von_compact(value: &VonValue) -> String {
    match value {
        VonValue::Null => "null".to_string(),
        VonValue::Bool(value) => value.to_string(),
        VonValue::Number(value) => value.to_string(),
        VonValue::String(value) => format!("\"{}\"", escape_string(value)),
        VonValue::Array(values) => {
            let body = values.iter().map(format_von_compact).collect::<Vec<_>>().join(", ");
            format!("[{}]", body)
        }
        VonValue::Object(values) => {
            let body =
                values.iter().map(|(key, value)| format!("{}: {}", format_key(key), format_von_compact(value))).collect::<Vec<_>>().join(", ");
            format!("{{{}}}", body)
        }
    }
}

pub fn format_von_pretty(value: &VonValue, indent: usize) -> String {
    match value {
        VonValue::Null | VonValue::Bool(_) | VonValue::Number(_) | VonValue::String(_) => format_von_compact(value),
        VonValue::Array(values) => {
            if values.is_empty() {
                return "[]".to_string();
            }
            let next_indent = indent + 4;
            let padding = " ".repeat(next_indent);
            let closing = " ".repeat(indent);
            let body =
                values.iter().map(|value| format!("{}{}", padding, format_von_pretty(value, next_indent))).collect::<Vec<_>>().join(",\n");
            format!("[\n{}\n{}]", body, closing)
        }
        VonValue::Object(values) => {
            if values.is_empty() {
                return "{}".to_string();
            }
            let next_indent = indent + 4;
            let padding = " ".repeat(next_indent);
            let closing = " ".repeat(indent);
            let body = values
                .iter()
                .map(|(key, value)| format!("{}{}: {}", padding, format_key(key), format_von_pretty(value, next_indent)))
                .collect::<Vec<_>>()
                .join(",\n");
            format!("{{\n{}\n{}}}", body, closing)
        }
    }
}

#[cfg(feature = "serde")]
pub fn to_string<T>(value: &T) -> Result<String, oak_core::OakError>
where
    T: serde::Serialize,
{
    oak_von::to_string(value)
}

#[cfg(feature = "serde")]
pub fn to_string_pretty<T>(value: &T) -> Result<String, oak_core::OakError>
where
    T: serde::Serialize,
{
    // 缩进排版待 `oak-von::formatter` 落地；当前与紧凑序列化相同。
    oak_von::to_string(value)
}

fn format_key(key: &str) -> String {
    if key.chars().next().is_some_and(is_identifier_start) && key.chars().all(is_identifier_continue) {
        key.to_string()
    }
    else {
        format!("\"{}\"", escape_string(key))
    }
}

fn escape_string(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '"' => "\\\"".to_string(),
            '\\' => "\\\\".to_string(),
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\t' => "\\t".to_string(),
            other => other.to_string(),
        })
        .collect()
}

fn is_identifier_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_identifier_continue(ch: char) -> bool {
    is_identifier_start(ch) || ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '@'
}

use crate::text::ToDocument;
use nyar_analyzer::format::{Document, FormatOptions};

impl ToDocument for VonValue {
    fn to_document(&self, options: &FormatOptions) -> Document {
        let text = if options.max_width == 0 { format_von_compact(self) } else { format_von_pretty(self, options.indent_width) };
        Document::text(text)
    }
}
