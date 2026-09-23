use crate::{state::ServerState, types::Position};
use oak_lsp::types::LocationRange;

pub struct ImplementationHandler;

impl ImplementationHandler {
    pub async fn handle(state: &ServerState, uri: &str, position: Position) -> Vec<LocationRange> {
        if let Some(symbol) = state.query_symbol_at_position(uri, position).await {
            let full_name = if symbol.namespace.is_empty() {
                symbol.name.clone()
            } else {
                format!("{}::{}", symbol.namespace, symbol.name)
            };

            match symbol.kind.as_str() {
                "interface" | "Interface" => {
                    return Self::find_trait_implementations(state, &full_name);
                }
                "class" | "Class" => {
                    return Self::find_class_children(state, &full_name);
                }
                "method" | "Method" => {
                    return Self::find_method_overrides(state, &symbol.name, &symbol.namespace);
                }
                _ => {}
            }
        }
        vec![]
    }

    fn find_trait_implementations(state: &ServerState, trait_full_name: &str) -> Vec<LocationRange> {
        let mut locations = Vec::new();

        if let Some(implementations) = state.index.get_trait_implementations(trait_full_name) {
            for impl_info in implementations {
                locations.push(LocationRange {
                    uri: impl_info.uri.clone().into(),
                    range: impl_info.range,
                });
            }
        }

        locations
    }

    fn find_class_children(state: &ServerState, class_full_name: &str) -> Vec<LocationRange> {
        let mut locations = Vec::new();

        if let Some(children) = state.index.get_class_children(class_full_name) {
            for child_full_name in children {
                if let Some(pos) = child_full_name.rfind("::") {
                    let ns = &child_full_name[..pos];
                    let name = &child_full_name[pos + 2..];
                    if let Some(ns_map) = state.index.symbols.get(ns) {
                        if let Some(symbol) = ns_map.get(name) {
                            locations.push(LocationRange {
                                uri: symbol.uri.clone().into(),
                                range: symbol.range.clone(),
                            });
                        }
                    }
                }
            }
        }

        locations
    }

    fn find_method_overrides(state: &ServerState, method_name: &str, _namespace: &str) -> Vec<LocationRange> {
        let mut locations = Vec::new();

        let method_suffix = if method_name.contains("::") {
            let pos = method_name.rfind("::").unwrap();
            &method_name[pos + 2..]
        } else {
            method_name
        };

        for ns_map in state.index.symbols.iter() {
            for symbol in ns_map.iter() {
                if symbol.kind == oak_lsp::types::SymbolKind::Method {
                    if symbol.name.ends_with(&format!("::{}", method_suffix)) {
                        locations.push(LocationRange {
                            uri: symbol.uri.clone().into(),
                            range: symbol.range.clone(),
                        });
                    }
                }
            }
        }

        locations
    }
}
