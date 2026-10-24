use crate::state::ServerState;
use core::range::Range;
use oak_lsp::types::*;

/// 代码操作处理器
pub struct CodeActionHandler;

impl CodeActionHandler {
    pub async fn handle(_state: &ServerState, _uri: &str, _range: Range<usize>) -> Vec<CodeAction> {
        let mut actions = Vec::new();

        // 基础代码操作示例：整理导入
        // 注意：这里暂时简化处理，不检查 only 参数
        actions.push(CodeAction {
            title: "Organize Imports".to_string(),
            kind: Some("source.organizeImports".to_string()),
            edit: Some(WorkspaceEdit { changes: std::collections::HashMap::new() }),
            diagnostics: None,
            command: None,
            is_preferred: Some(true),
            disabled: None,
        });

        actions
    }
}
