//! AWSL inlay hints (`<script>` span remap).

use super::widget::map_synthetic_span_to_file;
use crate::handlers::utils::make_source_position;
use crate::state::{DocumentState, ServerState};
use oak_lsp::types::*;
use oak_valkyrie::ast::{Expr, Item, Pattern, Statement};

pub struct AwslInlayHintHandler;

impl AwslInlayHintHandler {
    pub async fn handle(state: &ServerState, uri: &str) -> Vec<InlayHint> {
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

    fn map_span_end(doc: &DocumentState, synthetic_end: usize) -> Option<usize> {
        let base = doc.awsl_script_base?;
        let prefix = doc.awsl_synthetic_prefix?;
        Some(map_synthetic_span_to_file(base, prefix, synthetic_end..synthetic_end).start)
    }

    fn push_type_hint(
        doc: &DocumentState,
        synthetic_end: usize,
        label: String,
        hints: &mut Vec<InlayHint>,
    ) {
        let Some(file_offset) = Self::map_span_end(doc, synthetic_end) else {
            return;
        };
        let pos = doc.offset_to_position(file_offset);
        hints.push(InlayHint {
            position: make_source_position(
                file_offset,
                pos.line + 1,
                pos.character + 1,
                0,
            ),
            label,
            kind: Some(InlayHintKind::Type),
            tooltip: Some("Inferred type".to_string()),
            padding_left: Some(true),
            padding_right: None,
        });
    }

    async fn type_label_for_pattern(
        state: &ServerState,
        uri: &str,
        doc: &DocumentState,
        pattern: &Pattern,
    ) -> String {
        let span = Self::pattern_span(pattern);
        let Some(file_offset) = Self::map_span_end(doc, span.start) else {
            return ": Unknown".to_string();
        };
        let position = doc.offset_to_position(file_offset);
        if let Some(info) = state.query_awsl_script_symbol_at_position(uri, position).await {
            if let Some(ty) = info.type_info {
                return format!(": {}", ty.replace("var ", "").replace("function ", ""));
            }
        }
        ": Unknown".to_string()
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
                    let label = Self::type_label_for_pattern(state, uri, doc, pattern).await;
                    Self::push_type_hint(doc, span.end, label, hints);
                }
                Item::TypeFunction(func) => {
                    for param in &func.params {
                        if param.ty.is_none() {
                            Self::push_type_hint(doc, param.name.span.end, ": Any".to_string(), hints);
                        }
                    }
                    for stmt in &func.body.statements {
                        Self::collect_stmt_hints(stmt, doc, state, uri, hints).await;
                    }
                }
                Item::Micro(m) => {
                    for param in &m.params {
                        if param.ty.is_none() {
                            Self::push_type_hint(doc, param.name.span.end, ": Any".to_string(), hints);
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
                let label = Self::type_label_for_pattern(state, uri, doc, pattern).await;
                Self::push_type_hint(doc, span.end, label, hints);
            }
            Statement::ExprStmt { expr, .. } => {
                Self::collect_expr_hints(expr, doc, hints);
            }
        }
    }

    fn collect_expr_hints(expr: &Expr, doc: &DocumentState, hints: &mut Vec<InlayHint>) {
        match expr {
            Expr::Call { args, .. } => {
                for arg in args {
                    Self::collect_expr_hints(arg, doc, hints);
                }
            }
            _ => {}
        }
    }
}
