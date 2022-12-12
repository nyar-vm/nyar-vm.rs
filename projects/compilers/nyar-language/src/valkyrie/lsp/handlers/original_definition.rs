use crate::{state::ServerState, types::Position};
use oak_lsp::types::LocationRange;

pub struct OriginalDefinitionHandler;

impl OriginalDefinitionHandler {
    pub async fn handle(state: &ServerState, uri: &str, position: Position) -> Vec<LocationRange> {
        if let Some(symbol) = state.query_symbol_at_position(uri, position).await {
            let full_name = if symbol.namespace.is_empty() {
                symbol.name.clone()
            } else {
                format!("{}::{}", symbol.namespace, symbol.name)
            };

            if let Some(target_type) = state.index.get_type_alias_target(&full_name) {
                return Self::resolve_type_to_location(state, &target_type, &symbol.namespace);
            }

            if let Some(original_loc) = Self::find_original_through_usings(state, uri, &symbol.name, &symbol.namespace).await {
                return vec![original_loc];
            }
        }
        vec![]
    }

    async fn find_original_through_usings(
        state: &ServerState,
        uri: &str,
        symbol_name: &str,
        context_ns: &str,
    ) -> Option<LocationRange> {
        let doc = state.documents.get(uri)?;
        let ast = doc.ast.as_ref()?;

        let mut imported_from: Option<String> = None;
        for item in &ast.items {
            if let oak_valkyrie::ast::Item::Using(u) = item {
                let ns = u.path.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::");
                if let Some(ns_map) = state.index.symbols.get(&ns) {
                    if ns_map.contains_key(symbol_name) {
                        imported_from = Some(ns);
                        break;
                    }
                }
            }
        }

        if let Some(ns) = imported_from {
            if ns != context_ns {
                if let Some(ns_map) = state.index.symbols.get(&ns) {
                    if let Some(symbol) = ns_map.get(symbol_name) {
                        return Some(LocationRange {
                            uri: symbol.uri.clone().into(),
                            range: symbol.range.clone(),
                        });
                    }
                }
            }
        }

        None
    }

    fn resolve_type_to_location(state: &ServerState, type_name: &str, context_ns: &str) -> Vec<LocationRange> {
        if type_name.contains("::") {
            let parts: Vec<&str> = type_name.split("::").collect();
            if let Some(type_name) = parts.last() {
                let ns = parts[..parts.len() - 1].join("::");
                if let Some(ns_map) = state.index.symbols.get(&ns) {
                    if let Some(symbol) = ns_map.get(*type_name) {
                        return vec![LocationRange {
                            uri: symbol.uri.clone().into(),
                            range: symbol.range.clone(),
                        }];
                    }
                }
            }
        }

        if let Some(ns_map) = state.index.symbols.get(context_ns) {
            if let Some(symbol) = ns_map.get(type_name) {
                return vec![LocationRange {
                    uri: symbol.uri.clone().into(),
                    range: symbol.range.clone(),
                }];
            }
        }

        vec![]
    }
}
