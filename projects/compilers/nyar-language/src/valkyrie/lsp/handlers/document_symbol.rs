use crate::state::{DocumentState, ServerState};
use oak_core::UniversalElementRole;
use oak_lsp::types::{StructureItem, SymbolKind};
use oak_valkyrie::ast::{Item, Pattern, ValkyrieRoot as ProgramRoot};
use tracing::debug;

pub struct DocumentSymbolHandler;

impl DocumentSymbolHandler {
    /// 获取文档符号
    pub async fn handle(state: &ServerState, uri: &str) -> Vec<StructureItem> {
        debug!("Getting document symbols for: {}", uri);

        // 从 AST 中提取符号
        let doc = match state.get_document(uri) {
            Some(d) => d,
            None => return vec![],
        };
        let ast = match doc.ast.as_ref() {
            Some(a) => a,
            None => return vec![],
        };
        Self::extract_symbols_from_ast(ast, &doc)
    }

    /// 从 AST 中提取符号
    fn extract_symbols_from_ast(ast: &ProgramRoot, doc: &DocumentState) -> Vec<StructureItem> {
        let mut symbols = Vec::new();

        for item in &ast.items {
            if let Some(symbol) = Self::extract_symbol_from_item(item, doc) {
                symbols.push(symbol);
            }
        }

        symbols
    }

    fn extract_symbol_from_item(item: &Item, doc: &DocumentState) -> Option<StructureItem> {
        match item {
            Item::Namespace(ns) => {
                let mut children = Vec::new();
                for item in &ns.items {
                    if let Some(child) = Self::extract_symbol_from_item(item, doc) {
                        children.push(child);
                    }
                }
                Some(StructureItem {
                    name: ns.name.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::"),
                    detail: Some("namespace".to_string()),
                    role: UniversalElementRole::Container,
                    kind: SymbolKind::Namespace,
                    range: ns.span.clone(),
                    selection_range: ns.name.span.clone(),
                    children,
                    deprecated: false,
                })
            }
            Item::Using(imp) => Some(StructureItem {
                name: "import".to_string(),
                detail: Some(imp.path.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::")),
                role: UniversalElementRole::Metadata,
                kind: SymbolKind::Module,
                range: imp.span.clone(),
                selection_range: imp.path.span.clone(),
                children: vec![],
                deprecated: false,
            }),
            Item::Class(cls) => {
                let mut children = Vec::new();
                for item in &cls.items {
                    if let Some(child) = Self::extract_symbol_from_item(item, doc) {
                        children.push(child);
                    }
                }
                Some(StructureItem {
                    name: cls.name.name.clone(),
                    detail: Some("class".to_string()),
                    role: UniversalElementRole::Definition,
                    kind: SymbolKind::Class,
                    range: cls.span.clone(),
                    selection_range: cls.name.span.clone(),
                    children,
                    deprecated: false,
                })
            }
            Item::Widget(w) => {
                let mut children = Vec::new();
                for item in &w.items {
                    if let Some(child) = Self::extract_symbol_from_item(item, doc) {
                        children.push(child);
                    }
                }
                Some(StructureItem {
                    name: w.name.name.clone(),
                    detail: Some("widget".to_string()),
                    role: UniversalElementRole::Definition,
                    kind: SymbolKind::Class, // Use Class for Widget for now
                    range: w.span.clone(),
                    selection_range: w.name.span.clone(),
                    children,
                    deprecated: false,
                })
            }
            Item::TypeFunction(tf) => Some(StructureItem {
                name: tf.name.name.clone(),
                detail: Some("type function".to_string()),
                role: UniversalElementRole::Definition,
                kind: SymbolKind::Function,
                range: tf.span.clone(),
                selection_range: tf.name.span.clone(),
                children: vec![],
                deprecated: false,
            }),
            Item::Micro(m) => Some(StructureItem {
                name: m.name.name.clone(),
                detail: Some(format!("fn {}", m.name.name)),
                role: UniversalElementRole::Definition,
                kind: SymbolKind::Function,
                range: m.span.clone(),
                selection_range: m.name.span.clone(),
                children: vec![],
                deprecated: false,
            }),
            Item::Statement(s) => match s {
                oak_valkyrie::ast::Statement::Let { pattern, span, .. } => {
                    if let Pattern::Variable { name, .. } = pattern {
                        Some(StructureItem {
                            name: name.name.clone(),
                            detail: Some("variable".to_string()),
                            role: UniversalElementRole::Definition,
                            kind: SymbolKind::Variable,
                            range: span.clone(),
                            selection_range: name.span.clone(),
                            children: vec![],
                            deprecated: false,
                        })
                    }
                    else {
                        None
                    }
                }
                _ => None,
            },
            _ => None,
        }
    }
}
