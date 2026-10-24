use crate::state::ServerState;
use oak_lsp::types::{LocationRange, WorkspaceSymbol};
use tracing::debug;

/// 工作区符号处理器
pub struct WorkspaceSymbolHandler;

impl WorkspaceSymbolHandler {
    /// 处理工作区符号查询
    pub async fn handle(state: &ServerState, query: &str) -> Vec<WorkspaceSymbol> {
        let query_lower = query.to_lowercase();
        debug!("Workspace symbol query: {}", query);

        let mut result = Vec::new();

        // 遍历所有索引的符号
        for ns_map_ref in state.index.symbols.iter() {
            for symbol_ref in ns_map_ref.value().iter() {
                let symbol = symbol_ref.value();
                if query.is_empty() || symbol.name.to_lowercase().contains(&query_lower) {
                    result.push(WorkspaceSymbol {
                        name: symbol.name.clone(),
                        kind: symbol.kind,
                        location: LocationRange { uri: symbol.uri.clone().into(), range: symbol.range.clone() },
                        container_name: Some(ns_map_ref.key().clone()),
                    });
                }
            }
        }

        result
    }
}
