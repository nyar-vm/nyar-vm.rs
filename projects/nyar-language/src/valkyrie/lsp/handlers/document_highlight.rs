use super::utils::span_to_lsp_range;
use crate::{
    state::{DocumentState, ServerState},
    types::Position,
};
use oak_lsp::types::*;
use oak_valkyrie::ast::*;

/// 文档高亮处理器
pub struct DocumentHighlightHandler;

impl DocumentHighlightHandler {
    pub async fn handle(state: &ServerState, uri: &str, position: Position) -> Vec<DocumentHighlight> {
        let doc = match state.get_document(uri) {
            Some(d) => d,
            None => return vec![],
        };
        let ast = match doc.ast.as_ref() {
            Some(a) => a,
            None => return vec![],
        };

        // 1. 找到当前位置的符号
        let symbol = match state.query_symbol_at_position(uri, position).await {
            Some(s) => s,
            None => return vec![],
        };
        let name = symbol.name;

        // 2. 在文档中搜索所有同名符号
        let mut highlights = Vec::new();
        Self::collect_highlights(&ast.items, &name, &doc, &mut highlights);

        highlights
    }

    pub fn collect_highlights(items: &[Item], name: &str, doc: &DocumentState, highlights: &mut Vec<DocumentHighlight>) {
        for item in items {
            match item {
                Item::TypeFunction(f) => {
                    if f.name.name == name {
                        highlights.push(DocumentHighlight {
                            range: span_to_lsp_range(f.name.span.clone()),
                            kind: Some(DocumentHighlightKind::Write)
                        });
                    }
                    Self::collect_highlights_in_block(&f.body, name, doc, highlights);
                }
                Item::Class(c) => {
                    if c.name.name == name {
                        highlights.push(DocumentHighlight {
                            range: span_to_lsp_range(c.name.span.clone()),
                            kind: Some(DocumentHighlightKind::Write)
                        });
                    }
                    Self::collect_highlights(&c.items, name, doc, highlights);
                }
                Item::Namespace(n) => {
                    for part in &n.name.parts {
                        if part.name == name {
                            highlights.push(DocumentHighlight {
                                range: span_to_lsp_range(part.span.clone()),
                                kind: Some(DocumentHighlightKind::Write)
                            });
                        }
                    }
                    Self::collect_highlights(&n.items, name, doc, highlights);
                }
                Item::Statement(stmt) => {
                    Self::collect_highlights_in_stmt(stmt, name, doc, highlights);
                }
                Item::Widget(w) => {
                    if w.name.name == name {
                        highlights.push(DocumentHighlight {
                            range: span_to_lsp_range(w.name.span.clone()),
                            kind: Some(DocumentHighlightKind::Write)
                        });
                    }
                    Self::collect_highlights(&w.items, name, doc, highlights);
                }
                Item::Micro(m) => {
                    if m.name.name == name {
                        highlights.push(DocumentHighlight {
                            range: span_to_lsp_range(m.name.span.clone()),
                            kind: Some(DocumentHighlightKind::Write)
                        });
                    }
                    Self::collect_highlights_in_block(&m.body, name, doc, highlights);
                }
                _ => {}
            }
        }
    }

    fn collect_highlights_in_stmt(stmt: &Statement, name: &str, doc: &DocumentState, highlights: &mut Vec<DocumentHighlight>) {
        match stmt {
            Statement::Let { pattern, expr, .. } => {
                if let Pattern::Variable { name: id, .. } = pattern {
                    if id.name == name {
                        highlights.push(DocumentHighlight {
                            range: span_to_lsp_range(id.span.clone()),
                            kind: Some(DocumentHighlightKind::Write)
                        });
                    }
                }
                Self::collect_highlights_in_expr(expr, name, doc, highlights);
            }
            Statement::ExprStmt { expr, .. } => {
                Self::collect_highlights_in_expr(expr, name, doc, highlights);
            }
        }
    }

    fn collect_highlights_in_block(block: &Block, name: &str, doc: &DocumentState, highlights: &mut Vec<DocumentHighlight>) {
        for stmt in &block.statements {
            Self::collect_highlights_in_stmt(stmt, name, doc, highlights);
        }
    }

    fn collect_highlights_in_expr(expr: &Expr, name: &str, doc: &DocumentState, highlights: &mut Vec<DocumentHighlight>) {
        match expr {
            Expr::Ident(id) => {
                if id.name == name {
                    highlights.push(DocumentHighlight {
                        range: span_to_lsp_range(id.span.clone()),
                        kind: Some(DocumentHighlightKind::Read)
                    });
                }
            }
            Expr::Path(path) => {
                for part in &path.parts {
                    if part.name == name {
                        highlights.push(DocumentHighlight {
                            range: span_to_lsp_range(part.span.clone()),
                            kind: Some(DocumentHighlightKind::Read)
                        });
                    }
                }
            }
            Expr::Unary { expr, .. } => {
                Self::collect_highlights_in_expr(expr, name, doc, highlights);
            }
            Expr::Binary { left, right, .. } => {
                Self::collect_highlights_in_expr(left, name, doc, highlights);
                Self::collect_highlights_in_expr(right, name, doc, highlights);
            }
            Expr::Call { callee, args, .. } => {
                Self::collect_highlights_in_expr(callee, name, doc, highlights);
                for arg in args {
                    Self::collect_highlights_in_expr(arg, name, doc, highlights);
                }
            }
            Expr::Field { receiver, field, .. } => {
                Self::collect_highlights_in_expr(receiver, name, doc, highlights);
                if field.name == name {
                    highlights.push(DocumentHighlight {
                        range: span_to_lsp_range(field.span.clone()),
                        kind: Some(DocumentHighlightKind::Read)
                    });
                }
            }
            Expr::Index { receiver, index, .. } => {
                Self::collect_highlights_in_expr(receiver, name, doc, highlights);
                Self::collect_highlights_in_expr(index, name, doc, highlights);
            }
            Expr::Paren { expr, .. } => {
                Self::collect_highlights_in_expr(expr, name, doc, highlights);
            }
            Expr::Block(block) => {
                Self::collect_highlights_in_block(block, name, doc, highlights);
            }
            Expr::Lambda(lambda) => {
                for param in &lambda.params {
                    if param.name.name == name {
                        highlights.push(DocumentHighlight {
                            range: span_to_lsp_range(param.name.span.clone()),
                            kind: Some(DocumentHighlightKind::Write)
                        });
                    }
                }
                Self::collect_highlights_in_block(&lambda.body, name, doc, highlights);
            }
            Expr::If { condition, then_branch, else_branch, .. } => {
                Self::collect_highlights_in_expr(condition, name, doc, highlights);
                Self::collect_highlights_in_block(then_branch, name, doc, highlights);
                if let Some(eb) = else_branch {
                    Self::collect_highlights_in_block(eb, name, doc, highlights);
                }
            }
            Expr::Match { scrutinee, arms, .. } => {
                Self::collect_highlights_in_expr(scrutinee, name, doc, highlights);
                for arm in arms {
                    Self::collect_highlights_in_pattern(&arm.pattern, name, doc, highlights);
                    if let Some(guard) = &arm.guard {
                        Self::collect_highlights_in_expr(guard, name, doc, highlights);
                    }
                    Self::collect_highlights_in_expr(&arm.body, name, doc, highlights);
                }
            }
            Expr::Loop { condition, body, .. } => {
                if let Some(cond) = condition {
                    Self::collect_highlights_in_expr(cond, name, doc, highlights);
                }
                Self::collect_highlights_in_block(body, name, doc, highlights);
            }
            Expr::Return { expr, .. } => {
                if let Some(e) = expr {
                    Self::collect_highlights_in_expr(e, name, doc, highlights);
                }
            }
            Expr::Break { expr, .. } => {
                if let Some(e) = expr {
                    Self::collect_highlights_in_expr(e, name, doc, highlights);
                }
            }
            Expr::Yield { expr, .. } => {
                if let Some(e) = expr {
                    Self::collect_highlights_in_expr(e, name, doc, highlights);
                }
            }
            Expr::Raise { expr, .. } => {
                Self::collect_highlights_in_expr(expr, name, doc, highlights);
            }
            Expr::Catch { expr, arms, .. } => {
                Self::collect_highlights_in_expr(expr, name, doc, highlights);
                for arm in arms {
                    Self::collect_highlights_in_pattern(&arm.pattern, name, doc, highlights);
                    if let Some(guard) = &arm.guard {
                        Self::collect_highlights_in_expr(guard, name, doc, highlights);
                    }
                    Self::collect_highlights_in_expr(&arm.body, name, doc, highlights);
                }
            }
            Expr::Object { callee, fields, .. } => {
                Self::collect_highlights_in_expr(callee, name, doc, highlights);
                for field in fields {
                    let _field_name = field.0.name.clone();
                    if field.0.name == name {
                        highlights.push(DocumentHighlight {
                            range: span_to_lsp_range(field.0.span.clone()),
                            kind: Some(DocumentHighlightKind::Read)
                        });
                    }
                    if let Some(value) = &field.1 {
                        Self::collect_highlights_in_expr(value, name, doc, highlights);
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_highlights_in_pattern(
        pattern: &Pattern,
        name: &str,
        _doc: &DocumentState,
        highlights: &mut Vec<DocumentHighlight>,
    ) {
        match pattern {
            Pattern::Variable { name: id, .. } => {
                if id.name == name {
                    highlights.push(DocumentHighlight {
                        range: span_to_lsp_range(id.span.clone()),
                        kind: Some(DocumentHighlightKind::Write)
                    });
                }
            }
            _ => {}
        }
    }
}
