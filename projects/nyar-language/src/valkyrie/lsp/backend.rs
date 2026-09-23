//! Valkyrie LSP Backend Implementation
//!
//! 基于 Nyar 编译器基础设施的 LSP 后端实现

use core::range::Range;
use oak_core::source::Source;
use oak_lsp::{service::LanguageService, types::*};
use oak_vfs::{MemoryVfs, Vfs, WritableVfs};
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, error};

use crate::{
    diagnostics::DiagnosticsManager,
    errors::{LspError, LspResult},
    handlers::{self, formatting::FormattingOptions},
    state::ServerState,
    types::Position,
};

/// Valkyrie LSP 语言服务
///
/// 这个结构体实现了 LSP 协议，并使用 Nyar 编译器基础设施
/// 来提供语言服务功能。
pub struct ValkyrieLanguageService<V: Vfs = MemoryVfs> {
    vfs: V,
    workspace: oak_lsp::workspace::WorkspaceManager,
    state: Arc<ServerState>,
    diagnostics: Arc<DiagnosticsManager>,
}

impl ValkyrieLanguageService<MemoryVfs> {
    pub fn new() -> Self {
        Self::with_vfs(MemoryVfs::new())
    }
}

impl<V: Vfs> ValkyrieLanguageService<V> {
    pub fn with_vfs(vfs: V) -> Self {
        Self {
            vfs,
            workspace: oak_lsp::workspace::WorkspaceManager::new(),
            state: Arc::new(ServerState::new()),
            diagnostics: Arc::new(DiagnosticsManager::new()),
        }
    }

    /// 自定义方法：获取 AST
    pub async fn get_ast(&self, params: Value) -> LspResult<Value> {
        let uri = params
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LspError::InvalidParams("Missing uri parameter".to_string()))?;

        debug!("Getting AST for {}", uri);

        match self.state.get_ast(uri) {
            Some(_ast) => Ok(Value::String("[AST serialization not implemented]".to_string())),
            None => Err(LspError::NotFound("AST not found".to_string())),
        }
    }

    /// 自定义方法：获取 HIR
    pub async fn get_hir(&self, params: Value) -> LspResult<Value> {
        let uri = params
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LspError::InvalidParams("Missing uri parameter".to_string()))?;

        debug!("Getting HIR for {}", uri);

        match self.state.get_hir(uri) {
            Some(hir) => {
                // 将 HIR 序列化为 JSON
                serde_json::to_value(&hir).map_err(|e| LspError::Serialization(e.to_string()))
            }
            None => Ok(Value::Null),
        }
    }

    /// 自定义方法：查询符号
    pub async fn query_symbol(&self, params: Value) -> LspResult<Value> {
        let uri = params
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LspError::InvalidParams("Missing uri parameter".to_string()))?;

        let position =
            params.get("position").ok_or_else(|| LspError::InvalidParams("Missing position parameter".to_string()))?;

        let position: Position = serde_json::from_value(position.clone())
            .map_err(|e| LspError::InvalidParams(format!("Invalid position: {}", e)))?;

        debug!("Querying symbol at {}:{}:{}", uri, position.line, position.character);

        match self.state.query_symbol_at_position(uri, position).await {
            Some(symbol_info) => serde_json::to_value(&symbol_info).map_err(|e| LspError::Serialization(e.to_string())),
            None => Ok(Value::Null),
        }
    }

    /// 自定义方法：跳转到原定义
    pub async fn get_original_definition(&self, params: Value) -> LspResult<Value> {
        let uri = params
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LspError::InvalidParams("Missing uri parameter".to_string()))?;

        let position =
            params.get("position").ok_or_else(|| LspError::InvalidParams("Missing position parameter".to_string()))?;

        let position: Position = serde_json::from_value(position.clone())
            .map_err(|e| LspError::InvalidParams(format!("Invalid position: {}", e)))?;

        debug!("Getting original definition at {}:{}:{}", uri, position.line, position.character);

        let locations = handlers::OriginalDefinitionHandler::handle(&self.state, uri, position).await;
        serde_json::to_value(&locations).map_err(|e| LspError::Serialization(e.to_string()))
    }

    /*
    /// 自定义方法：获取测试
    pub async fn get_tests(&self, params: Value) -> LspResult<Value> {
        let uri = params.get("uri").and_then(|v| v.as_str()).ok_or_else(|| LspError::InvalidParams("Missing uri parameter".to_string()))?;
        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri: Url::parse(uri).map_err(|e| LspError::InvalidParams(format!("Invalid URI: {}", e)))? },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        match handlers::TestHandler::handle_get_tests(&self.state, params).await {
            Some(tests) => serde_json::to_value(tests).map_err(|e| LspError::Serialization(e.to_string())),
            None => Ok(Value::Null),
        }
    }
    */

    /// 编译文档并更新诊断信息
    async fn compile_and_diagnose(&self, uri: &str, text: &str) {
        debug!("Compiling document: {}", uri);

        if let Err(e) = self.state.compile_document(uri, text) {
            error!("Failed to compile document {}: {}", uri, e);
        }
    }
}

impl<V: Vfs + WritableVfs + Send + Sync + 'static> LanguageService for ValkyrieLanguageService<V> {
    type Lang = oak_valkyrie::ValkyrieLanguage;
    type Vfs = V;

    fn vfs(&self) -> &Self::Vfs {
        &self.vfs
    }

    fn workspace(&self) -> &oak_lsp::workspace::WorkspaceManager {
        &self.workspace
    }

    fn hover(
        &self,
        uri: &str,
        range: core::range::Range<usize>,
    ) -> impl std::future::Future<Output = Option<Hover>> + Send + '_ {
        let uri = uri.to_string();
        async move {
            let position = self.offset_to_position(&uri, range.start).await?;
            handlers::HoverHandler::handle(&self.state, &uri, position).await
        }
    }

    fn completion(&self, uri: &str, offset: usize) -> impl std::future::Future<Output = Vec<CompletionItem>> + Send + '_ {
        let uri = uri.to_string();
        async move {
            let position = match self.offset_to_position(&uri, offset).await {
                Some(p) => p,
                None => return vec![],
            };
            handlers::CompletionHandler::handle(&self.state, &uri, position).await.unwrap_or_default()
        }
    }

    fn definition(
        &self,
        uri: &str,
        range: core::range::Range<usize>,
    ) -> impl std::future::Future<Output = Vec<LocationRange>> + Send + '_ {
        let uri = uri.to_string();
        async move {
            let position = match self.offset_to_position(&uri, range.start).await {
                Some(p) => p,
                None => return vec![],
            };
            handlers::DefinitionHandler::handle(&self.state, &uri, position).await
        }
    }

    fn type_definition(
        &self,
        uri: &str,
        range: core::range::Range<usize>,
    ) -> impl std::future::Future<Output = Vec<LocationRange>> + Send + '_ {
        let uri = uri.to_string();
        async move {
            let position = match self.offset_to_position(&uri, range.start).await {
                Some(p) => p,
                None => return vec![],
            };
            handlers::TypeDefinitionHandler::handle(&self.state, &uri, position).await
        }
    }

    fn implementation(
        &self,
        uri: &str,
        range: core::range::Range<usize>,
    ) -> impl std::future::Future<Output = Vec<LocationRange>> + Send + '_ {
        let uri = uri.to_string();
        async move {
            let position = match self.offset_to_position(&uri, range.start).await {
                Some(p) => p,
                None => return vec![],
            };
            handlers::ImplementationHandler::handle(&self.state, &uri, position).await
        }
    }

    fn references(
        &self,
        uri: &str,
        range: core::range::Range<usize>,
    ) -> impl std::future::Future<Output = Vec<LocationRange>> + Send + '_ {
        let uri = uri.to_string();
        async move {
            let position = match self.offset_to_position(&uri, range.start).await {
                Some(p) => p,
                None => return vec![],
            };
            handlers::ReferencesHandler::handle(&self.state, &uri, position).await
        }
    }

    fn document_symbols(&self, uri: &str) -> impl std::future::Future<Output = Vec<StructureItem>> + Send + '_ {
        let uri = uri.to_string();
        async move { handlers::DocumentSymbolHandler::handle(&self.state, &uri).await }
    }

    fn workspace_symbols(&self, query: String) -> impl std::future::Future<Output = Vec<WorkspaceSymbol>> + Send + '_ {
        async move { handlers::WorkspaceSymbolHandler::handle(&self.state, &query).await }
    }

    fn rename(
        &self,
        uri: &str,
        range: core::range::Range<usize>,
        new_name: String,
    ) -> impl std::future::Future<Output = Option<WorkspaceEdit>> + Send + '_ {
        let uri = uri.to_string();
        async move {
            let position = match self.offset_to_position(&uri, range.start).await {
                Some(p) => p,
                None => return None,
            };
            handlers::RenameHandler::handle(&self.state, &uri, position, new_name).await
        }
    }

    fn folding_ranges(&self, uri: &str) -> impl std::future::Future<Output = Vec<FoldingRange>> + Send + '_ {
        let uri = uri.to_string();
        async move { handlers::FoldingRangeHandler::handle(&self.state, &uri).await }
    }

    fn signature_help(
        &self,
        uri: &str,
        range: Range<usize>,
    ) -> impl std::future::Future<Output = Option<SignatureHelp>> + Send + '_ {
        let uri = uri.to_string();
        async move {
            let position = match self.offset_to_position(&uri, range.start).await {
                Some(p) => p,
                None => return None,
            };
            handlers::SignatureHelpHandler::handle(&self.state, &uri, position).await
        }
    }

    fn inlay_hint(&self, uri: &str, _range: Range<usize>) -> impl std::future::Future<Output = Vec<InlayHint>> + Send + '_ {
        let uri = uri.to_string();
        async move { handlers::InlayHintHandler::handle(&self.state, &uri).await }
    }

    fn document_highlight(
        &self,
        uri: &str,
        range: Range<usize>,
    ) -> impl std::future::Future<Output = Vec<DocumentHighlight>> + Send + '_ {
        let uri = uri.to_string();
        async move {
            let position = match self.offset_to_position(&uri, range.start).await {
                Some(p) => p,
                None => return vec![],
            };
            handlers::DocumentHighlightHandler::handle(&self.state, &uri, position).await
        }
    }

    fn formatting(&self, uri: &str) -> impl std::future::Future<Output = Vec<TextEdit>> + Send + '_ {
        let uri = uri.to_string();
        async move {
            let options = FormattingOptions {
                tab_size: 4,
                insert_spaces: true,
                trim_trailing_whitespace: None,
                insert_final_newline: None,
                trim_final_newlines: None,
            };
            handlers::FormattingHandler::handle(&self.state, &uri, options).await
        }
    }

    fn range_formatting(
        &self,
        uri: &str,
        range: Range<usize>,
    ) -> impl std::future::Future<Output = Vec<TextEdit>> + Send + '_ {
        let uri = uri.to_string();
        async move {
            let options = FormattingOptions {
                tab_size: 4,
                insert_spaces: true,
                trim_trailing_whitespace: None,
                insert_final_newline: None,
                trim_final_newlines: None,
            };
            handlers::FormattingHandler::handle_range(&self.state, &uri, range, options).await
        }
    }

    fn code_action(&self, uri: &str, range: Range<usize>) -> impl std::future::Future<Output = Vec<CodeAction>> + Send + '_ {
        let uri = uri.to_string();
        async move { handlers::CodeActionHandler::handle(&self.state, &uri, range).await }
    }

    fn semantic_tokens(&self, uri: &str) -> impl std::future::Future<Output = Option<SemanticTokens>> + Send + '_ {
        let uri = uri.to_string();
        async move { handlers::SemanticTokensHandler::handle_full(&self.state, &uri).await }
    }

    fn diagnostics(&self, uri: &str) -> impl std::future::Future<Output = Vec<Diagnostic>> + Send + '_ {
        let uri = uri.to_string();
        async move {
            let text = match self.vfs.get_source(&uri) {
                Some(t) => t,
                None => return vec![],
            };
            let source_text = text.get_text_from(0);
            // 每次获取诊断时先尝试编译
            if let Err(e) = self.state.compile_document(&uri, &source_text) {
                error!("Failed to compile document {} for diagnostics: {}", uri, e);
            }

            let compiler_diagnostics = self.state.get_diagnostics(&uri).unwrap_or_default();
            self.diagnostics.convert_to_lsp_diagnostics(&compiler_diagnostics, &source_text)
        }
    }
}

impl<V: Vfs> ValkyrieLanguageService<V> {
    async fn offset_to_position(&self, uri: &str, offset: usize) -> Option<Position> {
        let source = self.vfs.get_source(uri)?;
        let line_map = oak_vfs::LineMap::from_source(&source);
        let (line, col) = line_map.offset_to_line_col_utf16(&source, offset);
        Some(Position { line, character: col })
    }
}
