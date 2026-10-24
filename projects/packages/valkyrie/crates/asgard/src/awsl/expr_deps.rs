//! 模板表达式依赖分析（Solid 细粒度订阅）。

use super::lower::ScriptBinding;

/// 从 AWSL/JS 风格表达式中提取标识符（含函数调用名）。
pub fn extract_expr_idents(expr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_string = None::<char>;
    for ch in expr.chars() {
        if let Some(quote) = in_string {
            if ch == quote {
                in_string = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_string = Some(ch);
            current.clear();
            continue;
        }
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch);
            continue;
        }
        if !current.is_empty() {
            if !is_js_keyword(&current) {
                out.push(current.clone());
            }
            current.clear();
        }
    }
    if !current.is_empty() && !is_js_keyword(&current) {
        out.push(current);
    }
    out.sort();
    out.dedup();
    out
}

/// 表达式依赖的响应式 `let mut` 绑定名。
pub fn reactive_deps(expr: &str, bindings: &[ScriptBinding]) -> Vec<String> {
    let reactive_names: std::collections::BTreeSet<String> =
        bindings.iter().filter(|binding| binding.reactive).map(|binding| binding.name.clone()).collect();
    extract_expr_idents(expr).into_iter().filter(|name| reactive_names.contains(name)).collect()
}

fn is_js_keyword(word: &str) -> bool {
    matches!(word, "true" | "false" | "null" | "undefined" | "if" | "else" | "return" | "new" | "typeof" | "in" | "not" | "and" | "or")
}
#[cfg(test)]
#[path = "../../tests/path_wired/awsl_expr_deps.rs"]
mod awsl_expr_deps_tests;
