use crate::state::{DocumentState, ServerState};
use oak_lsp::types::*;
use oak_valkyrie::ast::{Item, Statement};

/// 选择范围处理器
pub struct SelectionRangeHandler;

impl SelectionRangeHandler {
    pub async fn handle(state: &ServerState, uri: &str, offsets: Vec<usize>) -> Vec<SelectionRange> {
        let doc = match state.get_document(uri) {
            Some(d) => d,
            None => return vec![],
        };
        let ast = match doc.ast.as_ref() {
            Some(a) => a,
            None => return vec![],
        };

        let mut result = Vec::new();
        for offset in offsets {
            let mut ranges = Vec::new();
            Self::collect_selection_ranges(&ast.items, offset, &doc, &mut ranges);

            if let Some(mut current) = ranges.pop() {
                while let Some(parent) = ranges.pop() {
                    current = SelectionRange { range: current.range, parent: Some(Box::new(parent)) };
                }
                result.push(current);
            }
        }

        result
    }

    fn collect_selection_ranges(items: &[Item], offset: usize, doc: &DocumentState, ranges: &mut Vec<SelectionRange>) {
        for item in items {
            let span = match item {
                Item::Statement(Statement::Let { span, .. }) => span.clone(),
                Item::Statement(Statement::ExprStmt { span, .. }) => span.clone(),
                Item::Namespace(n) => n.span.clone(),
                Item::Using(u) => u.span.clone(),
                Item::Class(c) => c.span.clone(),
                Item::Widget(w) => w.span.clone(),
                Item::TypeFunction(tf) => tf.span.clone(),
                Item::Micro(m) => m.span.clone(),
                _ => continue,
            };

            if offset >= span.start && offset <= span.end {
                ranges.push(SelectionRange { range: span.clone(), parent: None });

                match item {
                    Item::TypeFunction(f) => {
                        Self::collect_selection_ranges_in_block(&f.body, offset, doc, ranges);
                    }
                    Item::Micro(m) => {
                        Self::collect_selection_ranges_in_block(&m.body, offset, doc, ranges);
                    }
                    Item::Class(c) => {
                        Self::collect_selection_ranges(&c.items, offset, doc, ranges);
                    }
                    Item::Namespace(n) => {
                        Self::collect_selection_ranges(&n.items, offset, doc, ranges);
                    }
                    Item::Widget(w) => {
                        Self::collect_selection_ranges(&w.items, offset, doc, ranges);
                    }
                    Item::Using(_) => {}
                    Item::Statement(s) => {
                        Self::collect_selection_ranges_in_stmt(s, offset, doc, ranges);
                    }
                    _ => {}
                }
                break;
            }
        }
    }

    fn collect_selection_ranges_in_block(
        block: &oak_valkyrie::ast::Block,
        offset: usize,
        doc: &DocumentState,
        ranges: &mut Vec<SelectionRange>,
    ) {
        let span = block.span.clone();
        if offset >= span.start && offset <= span.end {
            ranges.push(SelectionRange { range: span, parent: None });
            for stmt in &block.statements {
                Self::collect_selection_ranges_in_stmt(stmt, offset, doc, ranges);
            }
        }
    }

    fn collect_selection_ranges_in_stmt(
        stmt: &Statement,
        offset: usize,
        doc: &DocumentState,
        ranges: &mut Vec<SelectionRange>,
    ) {
        let span = match stmt {
            Statement::Let { span, .. } => span.clone(),
            Statement::ExprStmt { span, .. } => span.clone(),
        };
        if offset >= span.start && offset <= span.end {
            ranges.push(SelectionRange { range: span, parent: None });
            match stmt {
                Statement::Let { expr, .. } => {
                    Self::collect_selection_ranges_in_expr(expr, offset, doc, ranges);
                }
                Statement::ExprStmt { expr, .. } => {
                    Self::collect_selection_ranges_in_expr(expr, offset, doc, ranges);
                }
            }
        }
    }

    fn collect_selection_ranges_in_expr(
        _expr: &oak_valkyrie::ast::Expr,
        _offset: usize,
        _doc: &DocumentState,
        _ranges: &mut Vec<SelectionRange>,
    ) {
        // TODO: 实现表达式的选择范围收集
    }
}
