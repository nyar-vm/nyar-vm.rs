use crate::{state::ServerState, types::Position};
use oak_lsp::types::LocationRange;

pub struct TypeDefinitionHandler;

impl TypeDefinitionHandler {
    pub async fn handle(state: &ServerState, uri: &str, position: Position) -> Vec<LocationRange> {
        if let Some(symbol) = state.query_symbol_at_position(uri, position).await {
            if let Some(type_info) = &symbol.type_info {
                if let Some(type_loc) = Self::resolve_type_location(state, type_info, &symbol.namespace).await {
                    return vec![type_loc];
                }
            }
            if let Some(sig) = &symbol.signature {
                if let Some(return_type) = &sig.return_type {
                    if let Some(type_loc) = Self::resolve_type_location(state, return_type, &symbol.namespace).await {
                        return vec![type_loc];
                    }
                }
            }
        }
        vec![]
    }

    async fn resolve_type_location(state: &ServerState, type_name: &str, context_ns: &str) -> Option<LocationRange> {
        let clean_type = type_name
            .trim_end_matches('?')
            .split('<')
            .next()
            .unwrap_or(type_name)
            .trim();

        if clean_type.is_empty() || Self::is_primitive_type(clean_type) {
            return None;
        }

        if clean_type.contains("::") {
            let parts: Vec<&str> = clean_type.split("::").collect();
            if let Some(type_name) = parts.last() {
                let ns = parts[..parts.len() - 1].join("::");
                if let Some(ns_map) = state.index.symbols.get(&ns) {
                    if let Some(symbol) = ns_map.get(*type_name) {
                        return Some(LocationRange {
                            uri: symbol.uri.clone().into(),
                            range: symbol.range.clone(),
                        });
                    }
                }
            }
        }

        if let Some(ns_map) = state.index.symbols.get(context_ns) {
            if let Some(symbol) = ns_map.get(clean_type) {
                return Some(LocationRange {
                    uri: symbol.uri.clone().into(),
                    range: symbol.range.clone(),
                });
            }
        }

        for ns_map in state.index.symbols.iter() {
            if let Some(symbol) = ns_map.get(clean_type) {
                return Some(LocationRange {
                    uri: symbol.uri.clone().into(),
                    range: symbol.range.clone(),
                });
            }
        }

        None
    }

    fn is_primitive_type(type_name: &str) -> bool {
        matches!(
            type_name,
            "i8" | "i16" | "i32" | "i64" | "i128" |
            "u8" | "u16" | "u32" | "u64" | "u128" |
            "f32" | "f64" |
            "bool" | "char" | "utf8" | "utf16" | "c_str" |
            "Self" | "self"
        )
    }
}
