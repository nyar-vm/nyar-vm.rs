use crate::transitional::cst::valkyrie::{
    ParseError,
    ast::TermExpression,
    lexer::string_delim::opening_quote_run,
    tgrammar::{TgRoot, parse_tgrammar_template},
};

/// 解析 `t"` / `t"""` 模板字面量 token 切片为 T-Grammar 节点树。
pub(super) fn parse_template_literal(raw: &str) -> Result<TgRoot, ParseError> {
    let inner = strip_template_string_body(raw)?;
    parse_tgrammar_template(inner).map_err(|error| ParseError::invalid_at(error.message, error.span))
}

/// 判断 token 切片是否为 `t"` 模板字面量。
pub(super) fn is_template_string_literal(raw: &str) -> bool {
    raw.starts_with('t') && opening_quote_run(raw, 1).is_some()
}

fn strip_template_string_body(raw: &str) -> Result<&str, ParseError> {
    if !raw.starts_with('t') {
        return Err(ParseError::invalid("expected `t\"` template literal"));
    }
    let rest = &raw[1..];
    let (_, quote_count) = opening_quote_run(rest, 0).ok_or_else(|| ParseError::invalid("expected `t\"` template literal"))?;
    let quote_len = quote_count;
    if rest.len() < quote_len * 2 {
        return Err(ParseError::invalid("unterminated template literal"));
    }
    if quote_count == 2 {
        return Ok("");
    }
    Ok(&rest[quote_len..rest.len() - quote_len])
}

impl<'a> super::Parser<'a> {
    pub(super) fn parse_template_literal_expression(&mut self, raw: &str, span: std::ops::Range<usize>) -> Result<TermExpression, ParseError> {
        let nodes = parse_template_literal(raw)?;
        Ok(TermExpression::Template { nodes, span })
    }
}
