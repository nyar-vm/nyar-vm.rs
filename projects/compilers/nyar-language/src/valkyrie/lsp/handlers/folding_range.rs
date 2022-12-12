use crate::state::{DocumentState, ServerState};
use oak_lsp::types::*;
use oak_valkyrie::ast::*;

pub struct FoldingRangeHandler;

impl FoldingRangeHandler {
    pub async fn handle(state: &ServerState, uri: &str) -> Vec<FoldingRange> {
        let doc = match state.get_document(uri) {
            Some(d) => d,
            None => return vec![],
        };
        let ast = match doc.ast.as_ref() {
            Some(a) => a,
            None => return vec![],
        };

        let mut ranges = Vec::new();
        Self::collect_folding_ranges(ast, &doc, &mut ranges);

        ranges
    }

    fn collect_folding_ranges(ast: &ValkyrieRoot, doc: &DocumentState, ranges: &mut Vec<FoldingRange>) {
        for item in &ast.items {
            Self::collect_from_item(item, doc, ranges);
        }
    }

    fn collect_from_item(item: &Item, _doc: &DocumentState, ranges: &mut Vec<FoldingRange>) {
        match item {
            Item::Class(cls) => {
                ranges.push(FoldingRange {
                    range: core::range::Range { start: cls.span.start as usize, end: cls.span.end as usize },
                    kind: None,
                });
                for member in &cls.items {
                    Self::collect_from_item(member, _doc, ranges);
                }
            }
            Item::TypeFunction(func) => {
                ranges.push(FoldingRange {
                    range: core::range::Range { start: func.body.span.start as usize, end: func.body.span.end as usize },
                    kind: None,
                });
            }
            Item::Namespace(ns) => {
                ranges.push(FoldingRange {
                    range: core::range::Range { start: ns.span.start as usize, end: ns.span.end as usize },
                    kind: None,
                });
                for member in &ns.items {
                    Self::collect_from_item(member, _doc, ranges);
                }
            }
            Item::Micro(m) => {
                ranges.push(FoldingRange {
                    range: core::range::Range { start: m.body.span.start as usize, end: m.body.span.end as usize },
                    kind: None,
                });
            }
            Item::Widget(w) => {
                ranges.push(FoldingRange {
                    range: core::range::Range { start: w.span.start as usize, end: w.span.end as usize },
                    kind: None,
                });
                for member in &w.items {
                    Self::collect_from_item(member, _doc, ranges);
                }
            }
            _ => {}
        }
    }
}
