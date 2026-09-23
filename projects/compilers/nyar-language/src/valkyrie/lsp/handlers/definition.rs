use crate::{state::ServerState, types::Position};
use crate::handlers;
use oak_lsp::types::LocationRange;

pub struct DefinitionHandler;

impl DefinitionHandler {
    pub async fn handle(state: &ServerState, uri: &str, position: Position) -> Vec<LocationRange> {
        if handlers::awsl::document::is_awsl_uri(uri) {
            return handlers::awsl::AwslDefinitionHandler::handle(state, uri, position).await;
        }

        if let Some(symbol) = state.query_symbol_at_position(uri, position).await {
            return vec![symbol.location];
        }
        vec![]
    }
}
