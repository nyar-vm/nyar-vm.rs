//! AWSL source formatter.

use std_data::text::awsl::{
    AwslAttribute, AwslAttributeValue, AwslDirective, AwslDirectiveKind, AwslElement, AwslImport, AwslRoot, AwslTemplateNode, AwslTextPart,
};

use crate::formatter::{FormatBuffer, FormatError, FormatOptions, FormattedOutput};

pub(crate) fn format_awsl(source: &str, options: &FormatOptions) -> Result<FormattedOutput, FormatError> {
    crate::text::awsl::format_awsl_cst(source, options)
}

/// 基于已解析的 AWSL 根节点走 CST formatter 路径。
pub(crate) fn format_root(root: &AwslRoot, options: &FormatOptions) -> String {
    let mut buf = FormatBuffer::new(options);
    write_root(&mut buf, root);
    buf.finish()
}

fn write_root(buf: &mut FormatBuffer, root: &AwslRoot) {
    for import in &root.imports {
        write_import(buf, import);
        buf.newline();
    }
    if !root.imports.is_empty() && (root.has_widget_shell || root.script.is_some() || root.style.is_some()) {
        buf.newline();
    }

    // `<widget>` / `<template>` 容器：仅含模板子节点。
    // `<script>` 和 `<style>` 是 `<widget>` 的顶层兄弟，不可嵌套在内部。
    if root.has_widget_shell {
        if let Some(name) = &root.widget_name {
            buf.write(&format!("<widget {name}>"));
        }
        else {
            buf.write("<widget>");
        }
        buf.newline();

        for node in &root.template {
            if !is_significant_node(node) {
                continue;
            }
            write_node(buf, node);
            buf.newline();
        }

        buf.write("</widget>");
        buf.newline();
    }

    if let Some(script) = &root.script {
        buf.write("<script>");
        buf.newline();
        write_block_text(buf, script, true);
        buf.write("</script>");
        buf.newline();
    }
    if let Some(style) = &root.style {
        buf.write("<style>");
        buf.newline();
        write_block_text(buf, style, false);
        buf.newline();
        buf.write("</style>");
        buf.newline();
    }
}

fn write_import(buf: &mut FormatBuffer, import: &AwslImport) {
    buf.write(&format!("<import:{} from=\"{}\" />", import.name, escape_attr(&import.from)));
}

fn write_block_text(buf: &mut FormatBuffer, text: &str, brace_aware: bool) {
    let trimmed = text.trim_matches(|c| c == '\r' || c == '\n');
    if trimmed.is_empty() {
        return;
    }

    let mut lines: Vec<&str> = trimmed.lines().collect();
    while matches!(lines.first(), Some(line) if line.trim().is_empty()) {
        lines.remove(0);
    }
    while matches!(lines.last(), Some(line) if line.trim().is_empty()) {
        lines.pop();
    }

    let mut min_indent: usize = usize::MAX;
    for line in &lines {
        if line.trim().is_empty() {
            continue;
        }
        let leading = line.chars().take_while(|c| *c == ' ').count();
        min_indent = min_indent.min(leading);
    }
    if min_indent == usize::MAX {
        min_indent = 0;
    }

    let mut depth: usize = 0;
    for raw in lines {
        let line = if raw.len() >= min_indent { &raw[min_indent..] } else { raw.trim_start() };
        // brace_aware 模式下由 depth 重新计算缩进，需去除行首原始空格避免双重缩进。
        let content = if brace_aware { line.trim() } else { line.trim_end() };
        if content.is_empty() {
            buf.newline();
        }
        else {
            let starts_closing = brace_aware && starts_with_closing_token(content);
            if starts_closing {
                depth = depth.saturating_sub(1);
            }
            if brace_aware {
                buf.write(&" ".repeat(depth * 4));
            }
            buf.write(content);
            buf.newline();
            if brace_aware {
                let (open, close) = count_braces(content);
                // `starts_closing` 已为行首闭合 token 扣减 1 层深度，
                // 因此此处只统计剩余括号：close 需扣除已消耗的那一个。
                let effective_close = if starts_closing { close.saturating_sub(1) } else { close };
                if open > effective_close {
                    depth += open - effective_close;
                }
                else if effective_close > open {
                    depth = depth.saturating_sub(effective_close - open);
                }
            }
        }
    }
}

fn starts_with_closing_token(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('}') || trimmed.starts_with(']') || trimmed.starts_with(')')
}

fn count_braces(line: &str) -> (usize, usize) {
    let mut open = 0usize;
    let mut close = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    for ch in line.chars() {
        if escape {
            escape = false;
            continue;
        }
        if ch == '\\' {
            escape = true;
            continue;
        }
        if ch == '\'' && !in_double {
            in_single = !in_single;
            continue;
        }
        if ch == '"' && !in_single {
            in_double = !in_double;
            continue;
        }
        if in_single || in_double {
            continue;
        }
        match ch {
            '{' => open += 1,
            '}' => close += 1,
            _ => {}
        }
    }
    (open, close)
}

fn write_node(buf: &mut FormatBuffer, node: &AwslTemplateNode) {
    match node {
        AwslTemplateNode::Text { content, .. } => {
            let text = content.trim();
            if !text.is_empty() {
                buf.write(text);
            }
        }
        AwslTemplateNode::Interpolation { expr, .. } => {
            buf.write(&format!("{{{}}}", expr.trim()));
        }
        AwslTemplateNode::Element(el) => write_element(buf, el),
    }
}

fn write_element(buf: &mut FormatBuffer, el: &AwslElement) {
    buf.write(&format!("<{}", el.tag));
    for attr in &el.attributes {
        buf.write(" ");
        write_attribute(buf, attr);
    }
    for dir in &el.directives {
        buf.write(" ");
        write_directive(buf, dir);
    }
    if el.self_closing && el.children.is_empty() {
        buf.write(" />");
        return;
    }
    buf.write(">");
    if el.children.is_empty() {
        buf.write(&format!("</{}>", el.tag));
        return;
    }
    let inline = el.children.iter().all(|c| matches!(c, AwslTemplateNode::Text { .. } | AwslTemplateNode::Interpolation { .. }));
    if inline && el.children.len() <= 3 {
        for child in &el.children {
            write_node(buf, child);
        }
        buf.write(&format!("</{}>", el.tag));
        return;
    }
    buf.newline();
    buf.indent();
    for child in &el.children {
        if !is_significant_node(child) {
            continue;
        }
        write_node(buf, child);
        buf.newline();
    }
    buf.dedent();
    buf.write(&format!("</{}>", el.tag));
}

fn is_significant_node(node: &AwslTemplateNode) -> bool {
    match node {
        AwslTemplateNode::Text { content, .. } => !content.trim().is_empty(),
        _ => true,
    }
}

fn write_attribute(buf: &mut FormatBuffer, attr: &AwslAttribute) {
    match &attr.value {
        AwslAttributeValue::Literal(value) => {
            buf.write(&format!("{}=\"{}\"", attr.name, escape_attr(value)));
        }
        AwslAttributeValue::Expression(expr) => {
            buf.write(&format!(":{}=\"{}\"", attr.name, escape_attr(expr.trim())));
        }
        AwslAttributeValue::Mixed(parts) => {
            buf.write(&format!(":{}=\"", attr.name));
            for part in parts {
                match part {
                    AwslTextPart::Text(t) => buf.write_raw(&escape_attr(t)),
                    AwslTextPart::Expr(e) => buf.write_raw(&format!("{{{}}}", e.trim())),
                }
            }
            buf.write("\"");
        }
    }
}

fn write_directive(buf: &mut FormatBuffer, dir: &AwslDirective) {
    match &dir.kind {
        AwslDirectiveKind::If => write_quoted_directive(buf, "@if", dir),
        AwslDirectiveKind::Loop => write_quoted_directive(buf, "@loop", dir),
        AwslDirectiveKind::Style => write_quoted_directive(buf, "@style", dir),
        AwslDirectiveKind::Bind => write_quoted_directive(buf, "@bind", dir),
        AwslDirectiveKind::On(event) => write_quoted_directive(buf, &format!("@{event}"), dir),
        AwslDirectiveKind::Ref => write_quoted_directive(buf, "@ref", dir),
        AwslDirectiveKind::Class => write_quoted_directive(buf, "@class", dir),
        AwslDirectiveKind::Other(name) => write_quoted_directive(buf, &format!("@{name}"), dir),
    }
}

fn write_quoted_directive(buf: &mut FormatBuffer, name: &str, dir: &AwslDirective) {
    match &dir.value {
        Some(value) => buf.write(&format!("{name}=\"{}\"", escape_attr(value.trim()))),
        None => buf.write(name),
    }
}

fn escape_attr(value: &str) -> String {
    value.replace('&', "&amp;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::format_awsl;
    use crate::formatter::FormatOptions;

    #[test]
    fn format_script_block_normalizes_brace_indentation() {
        let source = r#"<widget back_top>
</widget>
<script>
micro handleScroll() {
    let y: i32 = 0
    if target !== "" {
        let el = document.querySelector(target)
        if el { y = el.scrollTop }
    } else {
    y = window.scrollY
}
if y >= parseInt(visibilityHeight) {
    visible = true
} else {
visible = false
}
}
</script>"#;

        let formatted = format_awsl(source, &FormatOptions::default()).expect("format should succeed").text;
        assert!(formatted.contains("    y = window.scrollY"));
        assert!(formatted.contains("    if y >= parseInt(visibilityHeight) {"));
        assert!(formatted.contains("        visible = false"));
    }
}
