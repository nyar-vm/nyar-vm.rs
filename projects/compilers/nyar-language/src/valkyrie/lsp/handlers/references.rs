use super::utils::span_to_range_usize;
use crate::{
    state::{DocumentState, ServerState},
    types::Position,
};
use oak_lsp::types::LocationRange;
use oak_valkyrie::ast::*;

/// 引用处理器
pub struct ReferencesHandler;

impl ReferencesHandler {
    pub async fn handle(state: &ServerState, uri: &str, position: Position) -> Vec<LocationRange> {
        if handlers::awsl::document::is_awsl_uri(uri) {
            return handlers::awsl::AwslReferencesHandler::handle(state, uri, position).await;
        }

        // 1. 找到当前位置的符号
        let symbol = match state.query_symbol_at_position(uri, position).await {
            Some(s) => s,
            None => return vec![],
        };
        let name = symbol.name;

        let mut locations = Vec::new();

        // 2. 遍历所有打开的文档，在 AST 中搜索引用
        for doc_ref in state.documents.iter() {
            let doc_uri = doc_ref.key();
            let doc_state = doc_ref.value();
            if let Some(ast) = &doc_state.ast {
                Self::collect_references(&ast.items, &name, doc_uri, doc_state, &mut locations);
            }
        }

        locations
    }

    fn collect_references(items: &[Item], name: &str, uri: &str, _doc: &DocumentState, locations: &mut Vec<LocationRange>) {
        for item in items {
            match item {
                Item::TypeFunction(f) => {
                    if f.name.name == name {
                        locations.push(LocationRange {
                            uri: uri.to_string().into(),
                            range: span_to_range_usize(f.name.span.clone()),
                        });
                    }
                    Self::collect_references_in_block(&f.body, name, uri, locations);
                }
                Item::Class(c) => {
                    if c.name.name == name {
                        locations.push(LocationRange {
                            uri: uri.to_string().into(),
                            range: span_to_range_usize(c.name.span.clone()),
                        });
                    }
                    Self::collect_references(&c.items, name, uri, _doc, locations);
                }
                Item::Namespace(n) => {
                    for part in &n.name.parts {
                        if part.name == name {
                            locations.push(LocationRange {
                                uri: uri.to_string().into(),
                                range: span_to_range_usize(part.span.clone()),
                            });
                        }
                    }
                    Self::collect_references(&n.items, name, uri, _doc, locations);
                }
                Item::Statement(stmt) => {
                    Self::collect_references_in_stmt(stmt, name, uri, locations);
                }
                Item::Widget(w) => {
                    if w.name.name == name {
                        locations.push(LocationRange {
                            uri: uri.to_string().into(),
                            range: span_to_range_usize(w.name.span.clone()),
                        });
                    }
                    Self::collect_references(&w.items, name, uri, _doc, locations);
                }
                Item::Micro(m) => {
                    if m.name.name == name {
                        locations.push(LocationRange {
                            uri: uri.to_string().into(),
                            range: span_to_range_usize(m.name.span.clone()),
                        });
                    }
                    Self::collect_references_in_block(&m.body, name, uri, locations);
                }
                _ => {}
            }
        }
    }

    fn collect_references_in_stmt(stmt: &Statement, name: &str, uri: &str, locations: &mut Vec<LocationRange>) {
        match stmt {
            Statement::Let { pattern, expr, .. } => {
                if let Pattern::Variable { name: id, .. } = pattern {
                    if id.name == name {
                        locations
                            .push(LocationRange { uri: uri.to_string().into(), range: span_to_range_usize(id.span.clone()) });
                    }
                }
                Self::collect_references_in_expr(expr, name, uri, locations);
            }
            Statement::ExprStmt { expr, .. } => {
                Self::collect_references_in_expr(expr, name, uri, locations);
            }
        }
    }

    fn collect_references_in_block(block: &Block, name: &str, uri: &str, locations: &mut Vec<LocationRange>) {
        for stmt in &block.statements {
            Self::collect_references_in_stmt(stmt, name, uri, locations);
        }
    }

    fn collect_references_in_expr(expr: &Expr, name: &str, uri: &str, locations: &mut Vec<LocationRange>) {
        match expr {
            Expr::Ident(id) => {
                if id.name == name {
                    locations.push(LocationRange { uri: uri.to_string().into(), range: span_to_range_usize(id.span.clone()) });
                }
            }
            Expr::Path(path) => {
                for part in &path.parts {
                    if part.name == name {
                        locations
                            .push(LocationRange { uri: uri.to_string().into(), range: span_to_range_usize(part.span.clone()) });
                    }
                }
            }
            Expr::Unary { expr, .. } => {
                Self::collect_references_in_expr(expr, name, uri, locations);
            }
            Expr::Binary { left, right, .. } => {
                Self::collect_references_in_expr(left, name, uri, locations);
                Self::collect_references_in_expr(right, name, uri, locations);
            }
            Expr::Call { callee, args, .. } => {
                Self::collect_references_in_expr(callee, name, uri, locations);
                for arg in args {
                    Self::collect_references_in_expr(arg, name, uri, locations);
                }
            }
            Expr::Field { receiver, field, .. } => {
                Self::collect_references_in_expr(receiver, name, uri, locations);
                if field.name == name {
                    locations
                        .push(LocationRange { uri: uri.to_string().into(), range: span_to_range_usize(field.span.clone()) });
                }
            }
            Expr::Index { receiver, index, .. } => {
                Self::collect_references_in_expr(receiver, name, uri, locations);
                Self::collect_references_in_expr(index, name, uri, locations);
            }
            Expr::Paren { expr, .. } => {
                Self::collect_references_in_expr(expr, name, uri, locations);
            }
            Expr::Block(block) => {
                Self::collect_references_in_block(block, name, uri, locations);
            }
            Expr::Lambda(lambda) => {
                for param in &lambda.params {
                    if param.name.name == name {
                        locations.push(LocationRange {
                            uri: uri.to_string().into(),
                            range: span_to_range_usize(param.name.span.clone()),
                        });
                    }
                }
                Self::collect_references_in_block(&lambda.body, name, uri, locations);
            }
            Expr::If { condition, then_branch, else_branch, .. } => {
                Self::collect_references_in_expr(condition, name, uri, locations);
                Self::collect_references_in_block(then_branch, name, uri, locations);
                if let Some(eb) = else_branch {
                    Self::collect_references_in_block(eb, name, uri, locations);
                }
            }
            Expr::Match { scrutinee, arms, .. } => {
                Self::collect_references_in_expr(scrutinee, name, uri, locations);
                for arm in arms {
                    Self::collect_references_in_pattern(&arm.pattern, name, uri, locations);
                    if let Some(guard) = &arm.guard {
                        Self::collect_references_in_expr(guard, name, uri, locations);
                    }
                    Self::collect_references_in_expr(&arm.body, name, uri, locations);
                }
            }
            Expr::Loop { condition, body, .. } => {
                if let Some(cond) = condition {
                    Self::collect_references_in_expr(cond, name, uri, locations);
                }
                Self::collect_references_in_block(body, name, uri, locations);
            }
            Expr::Return { expr, .. } => {
                if let Some(e) = expr {
                    Self::collect_references_in_expr(e, name, uri, locations);
                }
            }
            Expr::Break { expr, .. } => {
                if let Some(e) = expr {
                    Self::collect_references_in_expr(e, name, uri, locations);
                }
            }
            Expr::Yield { expr, .. } => {
                if let Some(e) = expr {
                    Self::collect_references_in_expr(e, name, uri, locations);
                }
            }
            Expr::Raise { expr, .. } => {
                Self::collect_references_in_expr(expr, name, uri, locations);
            }
            Expr::Catch { expr, arms, .. } => {
                Self::collect_references_in_expr(expr, name, uri, locations);
                for arm in arms {
                    Self::collect_references_in_pattern(&arm.pattern, name, uri, locations);
                    if let Some(guard) = &arm.guard {
                        Self::collect_references_in_expr(guard, name, uri, locations);
                    }
                    Self::collect_references_in_expr(&arm.body, name, uri, locations);
                }
            }
            Expr::Object { callee, fields, .. } => {
                Self::collect_references_in_expr(callee, name, uri, locations);
                for field in fields {
                    if field.0.name == name {
                        locations.push(LocationRange { uri: uri.to_string().into(), range: span_to_range_usize(field.0.span.clone()) });
                    }
                    if let Some(value) = &field.1 {
                        Self::collect_references_in_expr(value, name, uri, locations);
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_references_in_pattern(pattern: &Pattern, name: &str, uri: &str, locations: &mut Vec<LocationRange>) {
        match pattern {
            Pattern::Variable { name: id, .. } => {
                if id.name == name {
                    locations.push(LocationRange { uri: uri.to_string().into(), range: span_to_range_usize(id.span.clone()) });
                }
            }
            _ => {}
        }
    }
}
