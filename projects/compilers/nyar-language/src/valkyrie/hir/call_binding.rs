//! Positional / keyword call-argument binding (Python-style rules).

use crate::{
    NamePath,
    types::{
        Identifier, SourceID, SourceSpan,
        hir::{HirCallArgument, HirExpr, HirExprKind, HirParam, HirParameterBindingKind, HirVariadicKind},
    },
};

/// 调用实参与形参绑定错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallBindingError {
    /// 仅位置参数被按名传递。
    PositionalOnlyPassedByName {
        /// 参数名。
        name: Identifier,
    },
    /// 仅关键字参数被按位置传递。
    KeywordOnlyPassedPositionally {
        /// 参数名。
        name: Identifier,
    },
    /// 未知关键字参数。
    UnknownKeyword {
        /// 参数名。
        name: Identifier,
    },
    /// 重复的关键字参数。
    DuplicateKeyword {
        /// 参数名。
        name: Identifier,
    },
    /// 关键字参数之后出现位置参数。
    PositionalAfterKeyword,
    /// 位置参数数量超出允许范围。
    TooManyPositionalArguments,
    /// 缺少必需参数。
    MissingRequiredArgument {
        /// 参数名。
        name: Identifier,
    },
    /// 关键字可变参数之后出现多余关键字。
    UnexpectedKeywordAfterKeywordRest,
}

/// Bind call arguments to parameter slots in declaration order.
pub fn bind_call_arguments(params: &[HirParam], args: &[HirCallArgument]) -> Result<Vec<HirExpr>, CallBindingError> {
    let positional_rest_index = params.iter().position(|param| param.variadic == HirVariadicKind::PositionalRest);
    let keyword_rest_index = params.iter().position(|param| param.variadic == HirVariadicKind::KeywordRest);

    let mut bound: Vec<Option<HirExpr>> = vec![None; params.len()];
    let mut positional_cursor = 0usize;
    let mut saw_keyword = false;
    let mut overflow_positional: Vec<HirExpr> = Vec::new();
    let mut overflow_keyword: Vec<(Identifier, HirExpr)> = Vec::new();

    for arg in args {
        if let Some(name) = &arg.name {
            saw_keyword = true;
            if let Some(index) = params.iter().position(|param| param.name.name == *name && param.variadic != HirVariadicKind::KeywordRest) {
                let param = &params[index];
                if param.binding_kind == HirParameterBindingKind::PositionalOnly {
                    return Err(CallBindingError::PositionalOnlyPassedByName { name: name.clone() });
                }
                if bound[index].is_some() {
                    return Err(CallBindingError::DuplicateKeyword { name: name.clone() });
                }
                bound[index] = Some(arg.value.clone());
            }
            else if keyword_rest_index.is_some() {
                overflow_keyword.push((name.clone(), arg.value.clone()));
            }
            else {
                return Err(CallBindingError::UnknownKeyword { name: name.clone() });
            }
        }
        else {
            if saw_keyword {
                return Err(CallBindingError::PositionalAfterKeyword);
            }
            if let Some(rest_index) = positional_rest_index {
                if let Some(index) = next_positional_slot(params, &bound, positional_cursor, Some(rest_index)) {
                    if params[index].binding_kind == HirParameterBindingKind::KeywordOnly {
                        return Err(CallBindingError::KeywordOnlyPassedPositionally { name: params[index].name.name.clone() });
                    }
                    bound[index] = Some(arg.value.clone());
                    positional_cursor = index + 1;
                }
                else {
                    overflow_positional.push(arg.value.clone());
                }
                continue;
            }
            let index = next_positional_slot(params, &bound, positional_cursor, None).ok_or(CallBindingError::TooManyPositionalArguments)?;
            if params[index].binding_kind == HirParameterBindingKind::KeywordOnly {
                return Err(CallBindingError::KeywordOnlyPassedPositionally { name: params[index].name.name.clone() });
            }
            bound[index] = Some(arg.value.clone());
            positional_cursor = index + 1;
        }
    }

    if let Some(rest_index) = positional_rest_index {
        bound[rest_index] = Some(array_literal_expr(overflow_positional, params[rest_index].name.span.clone()));
    }
    else if !overflow_positional.is_empty() {
        return Err(CallBindingError::TooManyPositionalArguments);
    }

    if let Some(rest_index) = keyword_rest_index {
        if !overflow_keyword.is_empty() || bound[rest_index].is_none() {
            bound[rest_index] = Some(map_literal_expr(overflow_keyword, params[rest_index].name.span.clone()));
        }
    }
    else if !overflow_keyword.is_empty() {
        let (name, _) = overflow_keyword.into_iter().next().expect("non-empty keyword overflow");
        return Err(CallBindingError::UnknownKeyword { name });
    }

    let mut ordered = Vec::with_capacity(params.len());
    for (param, value) in params.iter().zip(bound.iter_mut()) {
        if value.is_none() {
            if let Some(default) = &param.default {
                *value = Some(default.clone());
            }
        }
        let Some(value) = value.take()
        else {
            return Err(CallBindingError::MissingRequiredArgument { name: param.name.name.clone() });
        };
        ordered.push(value);
    }
    Ok(ordered)
}

fn next_positional_slot(params: &[HirParam], bound: &[Option<HirExpr>], start: usize, positional_rest_index: Option<usize>) -> Option<usize> {
    params.iter().enumerate().skip(start).find_map(|(index, param)| {
        if bound[index].is_some() {
            return None;
        }
        if param.variadic == HirVariadicKind::KeywordRest {
            return None;
        }
        if param.variadic == HirVariadicKind::PositionalRest {
            return Some(index);
        }
        if positional_rest_index.is_some_and(|rest| index >= rest) {
            return None;
        }
        match param.binding_kind {
            HirParameterBindingKind::KeywordOnly => Some(index),
            HirParameterBindingKind::PositionalOnly | HirParameterBindingKind::PositionalOrKeyword => Some(index),
        }
    })
}

fn array_literal_expr(items: Vec<HirExpr>, span: SourceSpan) -> HirExpr {
    HirExpr { kind: HirExprKind::ArrayLiteral { items }, span }
}

fn map_literal_expr(entries: Vec<(Identifier, HirExpr)>, span: SourceSpan) -> HirExpr {
    HirExpr {
        kind: HirExprKind::Construct {
            path: NamePath::new(vec![Identifier::new("Map")]),
            args: entries
                .into_iter()
                .map(|(name, value)| HirExpr { kind: HirExprKind::FieldInit { name, value: Box::new(value) }, span: span.clone() })
                .collect(),
            resolved: None,
        },
        span,
    }
}
