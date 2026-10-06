use std::ops::Range;

use crate::transitional::cst::valkyrie::{
    ast::{
        IfLetStatement, IfStatement, LoopInStatement, LoopStatement, StringLiteral, StringSegment, TermAsExpression, TermBinaryExpression,
        TermCallExpression, TermDereferenceExpression, TermDotExpression, TermExpression, TermSubscriptExpression, TermUnaryExpression,
        TryStatement, TypeExpression, UntilNotStatement, UntilStatement, WhileLetStatement, WhileStatement,
    },
    lexer::{
        Lexer,
        string_delim::{MAX_QUOTE_COUNT, opening_quote_run, skip_string_literal},
    },
};

use super::{ParseError, Parser};

pub(super) fn parse_string_literal(raw: &str) -> Result<StringLiteral, ParseError> {
    let mut body = raw;
    let prefix = if body.starts_with('r') && opening_quote_run(body, 1).is_some() {
        body = &body[1..];
        Some("r".to_string())
    }
    else {
        None
    };

    let (delim, quote_count) = opening_quote_run(body, 0).ok_or_else(|| ParseError::invalid("字符串字面量缺少开引号"))?;
    if quote_count > MAX_QUOTE_COUNT {
        return Err(ParseError::invalid(format!("字符串引号层数超过 {MAX_QUOTE_COUNT}")));
    }
    let quote_len = quote_count;
    let delim_len = delim.len_utf8();
    if quote_count == 2 {
        if body.len() != quote_count * delim_len {
            return Err(ParseError::invalid("空字符串引号对不完整"));
        }
        return Ok(StringLiteral { prefix, quote_count: 2, segments: vec![StringSegment::Text(String::new())] });
    }
    if body.len() < quote_len * delim_len * 2 {
        return Err(ParseError::invalid("字符串字面量过短"));
    }

    let inner = &body[quote_len * delim_len..body.len() - quote_len * delim_len];
    let is_raw = prefix.is_some();

    let segments = if is_raw { vec![StringSegment::Text(inner.to_string())] } else { parse_cooked_string_segments(inner)? };

    Ok(StringLiteral { prefix, quote_count: quote_count as u8, segments })
}

/// 解析普通字符串中的文本与插值片段。
fn parse_cooked_string_segments(input: &str) -> Result<Vec<StringSegment>, ParseError> {
    let mut segments = Vec::new();
    let mut text = String::new();
    let mut cursor = 0;

    while cursor < input.len() {
        if input[cursor..].starts_with('{') {
            // 尝试查找插值结束的 `}`；若找不到（如字面 `{`），视为普通字符。
            match find_interpolation_end(input, cursor + 1) {
                Ok(end) => {
                    let expression_source = input[cursor + 1..end].trim();
                    // 空插值 `{}` 视为字面文本（常用于 `format("{}", x)` 等格式化占位符）。
                    if expression_source.is_empty() {
                        text.push('{');
                        text.push('}');
                        cursor = end + '}'.len_utf8();
                        continue;
                    }
                    push_text_segment(&mut segments, &mut text);
                    // Invalid interpolation body (e.g. ASCII `{|}` in a charset map) → literal text.
                    // Matches the unmatched-`}` fallback below; do not force downstream to twist strings.
                    match parse_interpolation_expression(expression_source) {
                        Ok(expression) => {
                            segments.push(StringSegment::Interpolation { expression: Box::new(expression), is_fluent: false });
                        }
                        Err(_) => {
                            text.push('{');
                            text.push_str(expression_source);
                            text.push('}');
                        }
                    }
                    cursor = end + '}'.len_utf8();
                    continue;
                }
                Err(_) => {
                    // `find_interpolation_end` 失败时，将 `{` 视为字面字符。
                    text.push('{');
                    cursor += '{'.len_utf8();
                    continue;
                }
            }
        }

        if input[cursor..].starts_with('\\') {
            let (decoded, next_cursor) = decode_escape_sequence(input, cursor)?;
            text.push_str(&decoded);
            cursor = next_cursor;
            continue;
        }

        let Some(ch) = input[cursor..].chars().next()
        else {
            break;
        };
        text.push(ch);
        cursor += ch.len_utf8();
    }

    push_text_segment(&mut segments, &mut text);
    if segments.is_empty() {
        segments.push(StringSegment::Text(String::new()));
    }
    Ok(segments)
}

/// 解析插值表达式正文。
fn parse_interpolation_expression(source: &str) -> Result<TermExpression, ParseError> {
    if source.is_empty() {
        return Err(ParseError::invalid("字符串插值表达式不能为空"));
    }

    let tokens = Lexer::tokenize(source)?;
    let mut parser = Parser::new(source, tokens);
    let expression = parser.parse_expression_bp(0)?;
    if !parser.is_eof() {
        return Err(ParseError::invalid(format!("字符串插值表达式未完全解析: {}", source)));
    }
    Ok(expression)
}

/// 查找 `{` 对应的结束 `}`，支持嵌套 `{}` 与表达式中的字符串字面量。
fn find_interpolation_end(source: &str, mut cursor: usize) -> Result<usize, ParseError> {
    let mut brace_depth = 1usize;

    while cursor < source.len() {
        if let Some(next_cursor) = try_skip_nested_string_literal(source, cursor)? {
            cursor = next_cursor;
            continue;
        }

        let Some(ch) = source[cursor..].chars().next()
        else {
            break;
        };

        match ch {
            '{' => {
                brace_depth += 1;
            }
            '}' => {
                brace_depth -= 1;
                if brace_depth == 0 {
                    return Ok(cursor);
                }
            }
            _ => {}
        }

        cursor += ch.len_utf8();
    }

    Err(ParseError::invalid("字符串插值缺少结束的 '}'"))
}

/// 若当前位置是嵌套字符串字面量，则跳过整个字面量并返回新的偏移。
fn try_skip_nested_string_literal(source: &str, cursor: usize) -> Result<Option<usize>, ParseError> {
    if opening_quote_run(source, cursor).is_some()
        || source.get(cursor..).is_some_and(|text| (text.starts_with('r') || text.starts_with('t')) && opening_quote_run(text, 1).is_some())
    {
        let end = skip_string_literal(source, cursor).map_err(ParseError::invalid)?;
        return Ok(Some(end));
    }
    Ok(None)
}

/// 解码单个转义序列，返回解码结果与新的偏移。
fn decode_escape_sequence(input: &str, cursor: usize) -> Result<(String, usize), ParseError> {
    let rest = &input[cursor..];
    if !rest.starts_with('\\') {
        return Err(ParseError::invalid("内部错误：转义序列应以反斜杠开头"));
    }

    let mut chars = rest.chars();
    chars.next();
    let Some(escape) = chars.next()
    else {
        return Err(ParseError::invalid("不完整的转义序列"));
    };

    let next_cursor = cursor + '\\'.len_utf8() + escape.len_utf8();
    let decoded = match escape {
        'n' => "\n".to_string(),
        't' => "\t".to_string(),
        'r' => "\r".to_string(),
        '\\' => "\\".to_string(),
        '0' => "\0".to_string(),
        '{' => "{".to_string(),
        '}' => "}".to_string(),
        '\'' => "'".to_string(),
        '"' => return Err(ParseError::invalid("cooked 字符串不支持 \\\" 转义")),
        'u' => {
            let unicode_rest = &input[next_cursor..];
            let Some(hex_body) = unicode_rest.strip_prefix('{').and_then(|value| value.split_once('}').map(|(hex, _)| hex))
            else {
                return Err(ParseError::invalid("Unicode 转义需要使用 \\u{...} 形式"));
            };
            let hex_digits = hex_body.trim();
            let codepoint =
                u32::from_str_radix(hex_digits, 16).map_err(|_| ParseError::invalid(format!("无效的 Unicode 码点: {}", hex_digits)))?;
            let Some(ch) = char::from_u32(codepoint)
            else {
                return Err(ParseError::invalid(format!("无效的 Unicode 标量值: {}", hex_digits)));
            };
            let consumed = 2 + unicode_rest.find('}').unwrap() + 1;
            return Ok((ch.to_string(), cursor + consumed));
        }
        other => {
            return Err(ParseError::invalid(format!("不支持的转义序列 \\{}", other)));
        }
    };

    Ok((decoded, next_cursor))
}

/// 将暂存文本写回片段列表，并自动合并相邻文本片段。
fn push_text_segment(segments: &mut Vec<StringSegment>, text: &mut String) {
    if text.is_empty() {
        return;
    }

    if let Some(StringSegment::Text(existing)) = segments.last_mut() {
        existing.push_str(text);
        text.clear();
        return;
    }

    segments.push(StringSegment::Text(std::mem::take(text)));
}

pub(super) fn span(start: usize, end: usize) -> Range<usize> {
    start..end
}

pub(super) fn expression_requires_statement(expression: &TermExpression) -> bool {
    let _ = expression;
    false
}

pub(super) fn expression_can_participate_in_value_composition(expression: &TermExpression) -> bool {
    let _ = expression;
    true
}

pub(super) fn ensure_value_expression(expression: &TermExpression) -> Result<(), ParseError> {
    if expression_can_participate_in_value_composition(expression) {
        return Ok(());
    }
    Err(ParseError::invalid_at("该控制流表达式不能出现在参数、初始化器或其他值表达式位置", expression.span().clone()))
}

pub(super) fn with_term_span(expression: TermExpression, span: Range<usize>) -> TermExpression {
    match expression {
        TermExpression::Name { path, .. } => TermExpression::Name { path, span },
        TermExpression::Literal { literal, .. } => TermExpression::Literal { literal, span },
        TermExpression::Unary(term_unary) => {
            let TermUnaryExpression { operator, base: expr, .. } = *term_unary;
            TermExpression::Unary(Box::new(TermUnaryExpression { operator, base: expr, span }))
        }
        TermExpression::Binary(term_binary) => {
            let TermBinaryExpression { operator, lhs, rhs, .. } = *term_binary;
            TermExpression::Binary(Box::new(TermBinaryExpression { operator, lhs, rhs, span }))
        }
        TermExpression::Call(inner) => {
            let TermCallExpression { callee, args, .. } = *inner;
            TermExpression::Call(Box::new(TermCallExpression { callee, args, span }))
        }
        TermExpression::DotCall(inner) => {
            let TermDotExpression { base, caller, arguments, .. } = *inner;
            TermExpression::DotCall(Box::new(TermDotExpression { base, caller, arguments, span }))
        }
        TermExpression::Dereference(inner) => {
            let TermDereferenceExpression { base, kind, .. } = *inner;
            TermExpression::Dereference(Box::new(TermDereferenceExpression { base, kind, span }))
        }
        TermExpression::Subscript(inner) => {
            let TermSubscriptExpression { base, subscripts, kind, .. } = *inner;
            TermExpression::Subscript(Box::new(TermSubscriptExpression { base, subscripts, kind, span }))
        }
        TermExpression::Tuple { items, .. } => TermExpression::Tuple { items, span },
        TermExpression::Array { items, .. } => TermExpression::Array { items, span },
        TermExpression::As(term_as) => {
            let TermAsExpression { base: expr, target: ty, .. } = *term_as;
            TermExpression::As(Box::new(TermAsExpression { base: expr, target: ty, span }))
        }
        TermExpression::Is(term_is) => {
            let crate::transitional::cst::valkyrie::ast::TermIsExpression { base: expr, target: pattern, .. } = *term_is;
            TermExpression::Is(Box::new(crate::transitional::cst::valkyrie::ast::TermIsExpression { base: expr, target: pattern, span }))
        }
        TermExpression::Turbofish { expr, arguments, .. } => TermExpression::Turbofish { expr, arguments, span },
        TermExpression::Assign { target, value, .. } => TermExpression::Assign { target, value, span },
        TermExpression::Raise { value, .. } => TermExpression::Raise { value, span },
        TermExpression::Catch { expr, arms, .. } => TermExpression::Catch { expr, arms, span },
        TermExpression::PostfixMatch { base, arms, .. } => TermExpression::PostfixMatch { base, arms, span },
        TermExpression::PostfixCatch { base, arms, .. } => TermExpression::PostfixCatch { base, arms, span },
        TermExpression::TryPropagate { base, .. } => TermExpression::TryPropagate { base, span },
        TermExpression::MacroInvoke { path, args, .. } => TermExpression::MacroInvoke { path, args, span },
        TermExpression::If(if_stmt) => {
            let IfStatement { condition, then_body, else_body, .. } = *if_stmt;
            TermExpression::If(Box::new(IfStatement { condition, then_body, else_body, span }))
        }
        TermExpression::IfLet(if_let_stmt) => {
            let IfLetStatement { pattern, item, then_body, else_body, .. } = *if_let_stmt;
            TermExpression::IfLet(Box::new(IfLetStatement { pattern, item, then_body, else_body, span }))
        }
        TermExpression::Loop(loop_stmt) => {
            let LoopStatement { body, .. } = *loop_stmt;
            TermExpression::Loop(Box::new(LoopStatement { body, span }))
        }
        TermExpression::LoopIn(loop_in_stmt) => {
            let LoopInStatement { label, pattern, iterator, condition, body, .. } = *loop_in_stmt;
            TermExpression::LoopIn(Box::new(LoopInStatement { label, pattern, iterator, condition, body, span }))
        }
        TermExpression::While(while_stmt) => {
            let WhileStatement { label, condition, body, .. } = *while_stmt;
            TermExpression::While(Box::new(WhileStatement { label, condition, body, span }))
        }
        TermExpression::WhileLet(while_let_stmt) => {
            let WhileLetStatement { label, pattern, scrutinee, guard, body, .. } = *while_let_stmt;
            TermExpression::WhileLet(Box::new(WhileLetStatement { label, pattern, scrutinee, guard, body, span }))
        }
        TermExpression::Until(until_stmt) => {
            let UntilStatement { label, pattern, iterator, condition, body, .. } = *until_stmt;
            TermExpression::Until(Box::new(UntilStatement { label, pattern, iterator, condition, body, span }))
        }
        TermExpression::UntilNot(until_not_stmt) => {
            let UntilNotStatement { label, pattern, iterator, condition, body, .. } = *until_not_stmt;
            TermExpression::UntilNot(Box::new(UntilNotStatement { label, pattern, iterator, condition, body, span }))
        }
        TermExpression::Try(try_stmt) => {
            let TryStatement { is_optional, is_forced, result_type, body, .. } = *try_stmt;
            TermExpression::Try(Box::new(TryStatement { is_optional, is_forced, result_type, body, span }))
        }
        TermExpression::Match { scrutinee, arms, .. } => TermExpression::Match { scrutinee, arms, span },
        TermExpression::Construct { path, fields, .. } => TermExpression::Construct { path, fields, span },
        TermExpression::Lambda { params, return_type, body, .. } => TermExpression::Lambda { params, return_type, body, span },
        TermExpression::Block { body, is_unsafe, .. } => TermExpression::Block { body, is_unsafe, span },
        TermExpression::XmlMarkup { nodes, .. } => TermExpression::XmlMarkup { nodes, span },
        TermExpression::Template { nodes, .. } => TermExpression::Template { nodes, span },
        TermExpression::AnonymousClass { is_value_type, parents, body, .. } => {
            TermExpression::AnonymousClass { is_value_type, parents, body, span }
        }
    }
}

pub(super) fn with_type_span(expression: TypeExpression, span: Range<usize>) -> TypeExpression {
    match expression {
        TypeExpression::Path(mut path) => {
            path.span = span;
            TypeExpression::Path(path)
        }
        TypeExpression::Array { item, .. } => TypeExpression::Array { item, span },
        TypeExpression::Tuple { items, .. } => TypeExpression::Tuple { items, span },
        TypeExpression::Row { methods, .. } => TypeExpression::Row { methods, span },
        TypeExpression::Pointer { kind, item, .. } => TypeExpression::Pointer { kind, item, span },
        TypeExpression::Associated { name, ty, .. } => TypeExpression::Associated { name, ty, span },
        TypeExpression::Nullable { item, .. } => TypeExpression::Nullable { item, span },
        TypeExpression::Function { params, return_type, .. } => TypeExpression::Function { params, return_type, span },
        TypeExpression::Union { items, .. } => TypeExpression::Union { items, span },
        TypeExpression::Intersection { items, .. } => TypeExpression::Intersection { items, span },
        TypeExpression::FixedArray { item, length, .. } => TypeExpression::FixedArray { item, length, span },
    }
}
