//! Prepare-rename: resolve whether the position is on a renamable symbol.

use crate::{handlers, state::ServerState, types::Position};
use oak_lsp::types::Range;

/// Result of a prepare-rename request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrepareRenameResult {
    /// Rename is allowed for the identifier at `range`.
    Ready { range: Range<usize> },
    /// No rename target at the position.
    NotRenamable,
}

pub struct PrepareRenameHandler;

impl PrepareRenameHandler {
    pub async fn handle(state: &ServerState, uri: &str, position: Position) -> PrepareRenameResult {
        let references = if handlers::awsl::document::is_awsl_uri(uri) {
            handlers::awsl::AwslReferencesHandler::rename_locations(state, uri, position).await
        }
        else {
            handlers::ReferencesHandler::handle(state, uri, position)
                .await
                .into_iter()
                .map(|loc| (loc.uri.to_string(), loc.range))
                .collect()
        };

        if references.is_empty() {
            return PrepareRenameResult::NotRenamable;
        }

        let (_, range) = references.into_iter().next().expect("non-empty references");
        PrepareRenameResult::Ready { range }
    }
}
