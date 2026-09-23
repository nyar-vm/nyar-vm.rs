use super::utils::make_source_position;
use crate::state::{DocumentState, ServerState};
use oak_lsp::types::*;
use oak_valkyrie::ast::{Expr, Item, Pattern, Statement};

pub struct InlayHintHandler;

impl InlayHintHandler {
    pub async fn handle(state: &ServerState, uri: &str) -> Vec<InlayHint> {
        if crate::handlers::awsl::document::is_awsl_uri(uri) {
            return crate::handlers::awsl::AwslInlayHintHandler::handle(state, uri).await;
        }

        let doc = match state.get_document(uri) {
            Some(d) => d,
            None => return vec![],
        };
        let ast = match doc.ast.as_ref() {
            Some(a) => a,
            None => return vec![],
        };

        let mut hints = Vec::new();
        Self::collect_items_hints(&ast.items, &doc, state, uri, &mut hints).await;

        hints
    }

    fn pattern_span(pattern: &Pattern) -> &oak_valkyrie::ast::Span {
        match pattern {
            Pattern::Wildcard { span } => span,
            Pattern::Variable { span, .. } => span,
            Pattern::Literal { span, .. } => span,
            Pattern::Type { span, .. } => span,
            Pattern::Class { span, .. } => span,
            Pattern::Else { span } => span,
        }
    }

    #[async_recursion::async_recursion]
    async fn collect_items_hints(
        items: &[Item],
        doc: &DocumentState,
        state: &ServerState,
        uri: &str,
        hints: &mut Vec<InlayHint>,
    ) {
        for item in items {
            match item {
                Item::Statement(Statement::Let { pattern, .. }) => {
                    let span = Self::pattern_span(pattern);
                    let pos = doc.offset_to_position(span.end);
                    let line = pos.line + 1;
                    let column = pos.character + 1;

                    let label =
                        if let Some(info) = state.query_symbol_at_position(uri, doc.offset_to_position(span.start)).await {
                            if let Some(ty) = info.type_info {
                                format!(": {}", ty.replace("var ", "").replace("function ", ""))
                            }
                            else {
                                ": Unknown".to_string()
                            }
                        }
                        else {
                            ": Unknown".to_string()
                        };

                    hints.push(InlayHint {
                        position: make_source_position(span.end, line, column, span.end - span.start),
                        label,
                        kind: Some(InlayHintKind::Type),
                        tooltip: Some("Inferred type".to_string()),
                        padding_left: Some(true),
                        padding_right: None,
                    });
                }
                Item::TypeFunction(func) => {
                    for param in &func.params {
                        if param.ty.is_none() {
                            let pos = doc.offset_to_position(param.name.span.end);
                            let line = pos.line + 1;
                            let column = pos.character + 1;
                            hints.push(InlayHint {
                                position: make_source_position(
                                    param.name.span.end,
                                    line,
                                    column,
                                    param.name.span.end - param.name.span.start,
                                ),
                                label: ": Any".to_string(),
                                kind: Some(InlayHintKind::Type),
                                tooltip: Some("Implicit type".to_string()),
                                padding_left: Some(true),
                                padding_right: None,
                            });
                        }
                    }
                    for stmt in &func.body.statements {
                        Self::collect_stmt_hints(stmt, doc, state, uri, hints).await;
                    }
                }
                Item::Micro(m) => {
                    for param in &m.params {
                        if param.ty.is_none() {
                            let pos = doc.offset_to_position(param.name.span.end);
                            let line = pos.line + 1;
                            let column = pos.character + 1;
                            hints.push(InlayHint {
                                position: make_source_position(
                                    param.name.span.end,
                                    line,
                                    column,
                                    param.name.span.end - param.name.span.start,
                                ),
                                label: ": Any".to_string(),
                                kind: Some(InlayHintKind::Type),
                                tooltip: Some("Implicit type".to_string()),
                                padding_left: Some(true),
                                padding_right: None,
                            });
                        }
                    }
                    for stmt in &m.body.statements {
                        Self::collect_stmt_hints(stmt, doc, state, uri, hints).await;
                    }
                }
                Item::Class(cls) => {
                    Self::collect_items_hints(&cls.items, doc, state, uri, hints).await;
                }
                Item::Namespace(ns) => {
                    Self::collect_items_hints(&ns.items, doc, state, uri, hints).await;
                }
                Item::Widget(w) => {
                    Self::collect_items_hints(&w.items, doc, state, uri, hints).await;
                }
                _ => {}
            }
        }
    }

    async fn collect_stmt_hints(
        stmt: &Statement,
        doc: &DocumentState,
        state: &ServerState,
        uri: &str,
        hints: &mut Vec<InlayHint>,
    ) {
        match stmt {
            Statement::Let { pattern, .. } => {
                let span = Self::pattern_span(pattern);
                let pos = doc.offset_to_position(span.end);
                let line = pos.line + 1;
                let column = pos.character + 1;
                let label = if let Some(info) = state.query_symbol_at_position(uri, doc.offset_to_position(span.start)).await {
                    if let Some(ty) = info.type_info {
                        format!(": {}", ty.replace("var ", "").replace("function ", ""))
                    }
                    else {
                        ": Unknown".to_string()
                    }
                }
                else {
                    ": Unknown".to_string()
                };

                hints.push(InlayHint {
                    position: make_source_position(span.end, line, column, span.end - span.start),
                    label,
                    kind: Some(InlayHintKind::Type),
                    tooltip: Some("Inferred type".to_string()),
                    padding_left: Some(true),
                    padding_right: None,
                });
            }
            Statement::ExprStmt { expr, .. } => {
                Self::collect_expr_hints(expr, doc, state, uri, hints).await;
            }
        }
    }

    async fn collect_expr_hints(
        expr: &Expr,
        _doc: &DocumentState,
        _state: &ServerState,
        _uri: &str,
        _hints: &mut Vec<InlayHint>,
    ) {
        match expr {
            Expr::Call { args, .. } => for _arg in args {},
            _ => {}
        }
    }
}
