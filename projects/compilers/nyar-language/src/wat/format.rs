//! WAT 文本格式化。

use std_data::text::wat::WatDocument;

pub fn format_wat_document(document: &WatDocument) -> String {
    let mut result = String::from("(module");
    if let Some(module_name) = &document.module_name {
        result.push(' ');
        result.push_str(module_name);
    }

    if document.fields.is_empty() {
        result.push(')');
        return result;
    }

    for field in &document.fields {
        result.push('\n');
        result.push_str("  ");
        result.push_str(field.trim());
    }
    result.push('\n');
    result.push(')');
    result
}
