//! AWSL 属性表达式解析（三元、引号内 class token 等）。

/// 拆分 AWSL 风格三元表达式 `cond ? then : else`（尊重括号/引号嵌套）。
pub fn split_awsl_ternary(expr: &str) -> Option<(String, String, String)> {
    let mut depth_paren = 0i32;
    let mut depth_brace = 0i32;
    let mut in_string = None::<char>;
    let mut question = None::<usize>;
    let mut colon = None::<usize>;
    let mut index = 0usize;
    while index < expr.len() {
        let ch = expr[index..].chars().next()?;
        let ch_len = ch.len_utf8();
        if let Some(quote) = in_string {
            if ch == '\\' {
                index += ch_len + 1.min(expr.len().saturating_sub(index + ch_len));
                continue;
            }
            if ch == quote {
                in_string = None;
            }
            index += ch_len;
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_string = Some(ch);
            index += ch_len;
            continue;
        }
        match ch {
            '(' => depth_paren += 1,
            ')' => depth_paren -= 1,
            '{' => depth_brace += 1,
            '}' => depth_brace -= 1,
            '?' if depth_paren == 0 && depth_brace == 0 && question.is_none() => {
                question = Some(index);
            }
            ':' if depth_paren == 0 && depth_brace == 0 && question.is_some() && colon.is_none() => {
                colon = Some(index);
            }
            _ => {}
        }
        index += ch_len;
    }
    let q = question?;
    let c = colon?;
    let cond = expr[..q].trim().to_string();
    let then_arm = expr[q + 1..c].trim().to_string();
    let else_arm = expr[c + 1..].trim().to_string();
    if cond.is_empty() || then_arm.is_empty() {
        return None;
    }
    Some((cond, then_arm, else_arm))
}

/// 从表达式中保守提取可能为 Tailwind utility 的 token（静态字面量 + 三元各 arm）。
pub fn collect_utility_tokens_from_expr(expr: &str, sink: &mut Vec<String>) {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Some((_, then_arm, else_arm)) = split_awsl_ternary(trimmed) {
        collect_utility_tokens_from_expr(&then_arm, sink);
        collect_utility_tokens_from_expr(&else_arm, sink);
        return;
    }
    push_quoted_literals(trimmed, sink);
    if sink.is_empty() && !trimmed.contains('?') && !trimmed.contains('(') {
        push_whitespace_tokens(trimmed, sink);
    }
}

/// 将空白分隔的 class 字符串拆成 token 并入集合。
pub fn push_whitespace_tokens(text: &str, sink: &mut Vec<String>) {
    for token in text.split_whitespace() {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if is_utility_token(token) {
            sink.push(token.to_string());
        }
    }
}

fn push_quoted_literals(expr: &str, sink: &mut Vec<String>) {
    let mut index = 0usize;
    while index < expr.len() {
        let slice = &expr[index..];
        let Some(quote) = slice.chars().next().filter(|c| *c == '"' || *c == '\'')
        else {
            index += 1;
            continue;
        };
        let close = slice[1..].find(quote).map(|pos| pos + 1);
        let Some(end) = close
        else {
            break;
        };
        let inner = &slice[1..end];
        push_whitespace_tokens(inner, sink);
        index += end + 1;
    }
}

fn is_utility_token(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':' | '/' | '[' | ']'))
}
#[cfg(test)]
#[path = "../../tests/path_wired/awsl_expr_util.rs"]
mod awsl_expr_util_tests;
