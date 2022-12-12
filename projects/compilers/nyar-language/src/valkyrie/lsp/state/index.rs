//! LSP server state and AWSL ABI workspace index.

use std::sync::{Arc, RwLock};

use dashmap::DashMap;
use oak_lsp::types::LocationRange;
use std_data::text::awsl::{AbiIssue, ComponentAbi};

use super::cache::SemanticCache;
use super::document::DocumentState;
use super::symbol::{GlobalIndex, GlobalSymbol, SymbolInfo};
use crate::legion::LegionManager;

/// One AWSL component ABI entry in the workspace index.
#[derive(Debug, Clone)]
pub struct AwslAbiEntry {
    /// Owning document URI.
    pub uri: String,
    /// Widget name (`snake_case`).
    pub widget_name: String,
    /// Extracted component ABI.
    pub abi: ComponentAbi,
    /// `<script>` body start offset in the `.awsl` file.
    pub script_base_offset: usize,
}

/// Shared LSP server state.
#[derive(Clone)]
pub struct ServerState {
    /// Open / indexed documents.
    pub documents: Arc<DashMap<String, DocumentState>>,
    /// Global symbol index.
    pub index: Arc<GlobalIndex>,
    /// Semantic resolution cache.
    pub semantic_cache: Arc<SemanticCache>,
    /// Workspace root URI string.
    pub workspace_root: Arc<RwLock<Option<String>>>,
    /// Legion package manager.
    pub legion: Arc<RwLock<LegionManager>>,
    /// AWSL component ABI index (`widget_name` → entry).
    pub awsl_abi_index: Arc<DashMap<String, AwslAbiEntry>>,
}

impl ServerState {
    /// Create empty server state.
    pub fn new() -> Self {
        Self {
            documents: Arc::new(DashMap::new()),
            index: Arc::new(GlobalIndex::new()),
            semantic_cache: Arc::new(SemanticCache::new()),
            workspace_root: Arc::new(RwLock::new(None)),
            legion: Arc::new(RwLock::new(LegionManager::new())),
            awsl_abi_index: Arc::new(DashMap::new()),
        }
    }

    /// Get a document by URI.
    pub fn get_document(&self, uri: &str) -> Option<DocumentState> {
        self.documents.get(uri).map(|d| d.clone())
    }

    /// Cache a resolved symbol for a namespace.
    pub fn cache_symbol(&self, namespace: &str, name: &str, symbol: Arc<GlobalSymbol>) {
        use super::symbol::SymbolInfo;
        let info = SymbolInfo {
            name: symbol.name.clone(),
            namespace: symbol.namespace.clone(),
            kind: format!("{:?}", symbol.kind),
            type_info: None,
            documentation: symbol.documentation.clone(),
            location: LocationRange {
                uri: symbol.uri.clone().into(),
                range: symbol.range.clone(),
            },
            signature: None,
            class_info: None,
        };
        self.semantic_cache
            .cache
            .entry(namespace.to_string())
            .or_insert_with(DashMap::new)
            .insert(name.to_string(), Arc::new(info));
    }

    /// Update AWSL ABI index entry for one document.
    pub fn update_awsl_abi_entry(
        &self,
        uri: &str,
        widget_name: &str,
        abi: ComponentAbi,
        script_base_offset: usize,
    ) {
        self.awsl_abi_index.insert(
            widget_name.to_string(),
            AwslAbiEntry {
                uri: uri.to_string(),
                widget_name: widget_name.to_string(),
                abi,
                script_base_offset,
            },
        );
    }

    /// Remove AWSL ABI entries owned by a document.
    pub fn remove_awsl_abi_entries_for_uri(&self, uri: &str) {
        self.awsl_abi_index.retain(|_, entry| entry.uri != uri);
    }

    /// Lookup ABI by widget name.
    pub fn awsl_abi_for_widget(&self, widget_name: &str) -> Option<AwslAbiEntry> {
        self.awsl_abi_index.get(widget_name).map(|e| e.clone())
    }

    /// Re-validate cross-file ABI bindings for all open AWSL documents.
    pub fn refresh_awsl_cross_file_abi_diagnostics(&self) {
        use std_data::text::awsl::ComponentAbiIndex;

        let mut index = ComponentAbiIndex::new();
        for entry in self.awsl_abi_index.iter() {
            index.insert(entry.widget_name.clone(), entry.abi.clone());
        }

        for mut doc_ref in self.documents.iter_mut() {
            if !crate::handlers::awsl::document::is_awsl_uri(doc_ref.key()) {
                continue;
            }
            let Some(root) = doc_ref.awsl_root.clone() else {
                continue;
            };
            let cross_issues = index.validate_awsl_root(&root);
            doc_ref
                .diagnostics
                .retain(|d| !d.labels.iter().any(|l| {
                    matches!(l.key.as_deref(), Some("AWSL ABI cross-file") | Some("AWSL ABI lint"))
                }));
            for issue in cross_issues {
                let emit = matches!(issue.kind, std_data::text::awsl::AbiIssueKind::NotSnakeCase)
                    || issue.severity == std_data::text::awsl::AbiSeverity::Error;
                if !emit {
                    continue;
                }
                let source = doc_ref.file_id.unwrap_or_default();
                let mut diag = crate::handlers::naming::abi_issue_to_diagnostic(&issue, source, 0);
                if let Some(label) = diag.labels.first_mut() {
                    label.key = Some(if issue.kind == std_data::text::awsl::AbiIssueKind::NotSnakeCase {
                        "AWSL ABI lint".into()
                    } else {
                        "AWSL ABI cross-file".into()
                    });
                }
                doc_ref.diagnostics.push(diag);
            }
        }
    }
}

impl Default for ServerState {
    fn default() -> Self {
        Self::new()
    }
}
