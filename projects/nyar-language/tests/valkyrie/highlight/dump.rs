use nyar_analyzer::highlight::{HighlightKind, HighlightSpan};

/// 将 merged highlight spans 格式化为稳定的 `*.highlight` 文本快照。
pub fn dump_highlight_snapshot(source: &str, spans: &[HighlightSpan]) -> String {
    let mut lines = Vec::with_capacity(spans.len());
    for span in spans {
        let text = source[span.range.clone()].to_string();
        let modifier = match &span.modifier {
            Some(value) => format!("Some({})", escape_snapshot_text(value)),
            None => "null".to_string(),
        };
        lines.push(format!(
            "Span {{ kind: {}, span: {}..{}, text: {}, modifier: {} }}",
            highlight_kind_name(span.kind),
            span.range.start,
            span.range.end,
            escape_snapshot_text(&text),
            modifier
        ));
    }
    lines.join("\n")
}

fn highlight_kind_name(kind: HighlightKind) -> &'static str {
    match kind {
        HighlightKind::None => "None",
        HighlightKind::Keyword => "Keyword",
        HighlightKind::ControlKeyword => "ControlKeyword",
        HighlightKind::String => "String",
        HighlightKind::Number => "Number",
        HighlightKind::Comment => "Comment",
        HighlightKind::Operator => "Operator",
        HighlightKind::Punctuation => "Punctuation",
        HighlightKind::Identifier => "Identifier",
        HighlightKind::TypeIdentifier => "TypeIdentifier",
        HighlightKind::VariantIdentifier => "VariantIdentifier",
        HighlightKind::FunctionIdentifier => "FunctionIdentifier",
        HighlightKind::Parameter => "Parameter",
        HighlightKind::Property => "Property",
        HighlightKind::Field => "Field",
        HighlightKind::Variable => "Variable",
        HighlightKind::Constant => "Constant",
        HighlightKind::Namespace => "Namespace",
        HighlightKind::Module => "Module",
        HighlightKind::Decorator => "Decorator",
        HighlightKind::Regex => "Regex",
        HighlightKind::Escape => "Escape",
        HighlightKind::Delimiter => "Delimiter",
        HighlightKind::Interpolation => "Interpolation",
    }
}

fn escape_snapshot_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 2);
    escaped.push('"');
    for ch in text.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => {
                escaped.push_str(&format!("\\u{{{:04x}}}", ch as u32));
            }
            ch => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}
