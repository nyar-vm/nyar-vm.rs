//! LSP 服务器能力配置
//!
//! 定义 Valkyrie LSP 服务器支持的功能

use serde_json::Value;

/// 返回服务器支持的能力
pub fn server_capabilities() -> Value {
    serde_json::json!({
        "textDocumentSync": 1, // Full
        "hoverProvider": true,
        "completionProvider": {
            "resolveProvider": true,
            "triggerCharacters": [".", "::", "(", "[", "{"]
        },
        "definitionProvider": true,
        "typeDefinitionProvider": true,
        "implementationProvider": true,
        "referencesProvider": true,
        "documentHighlightProvider": true,
        "documentSymbolProvider": true,
        "workspaceSymbolProvider": true,
        "codeActionProvider": {
            "codeActionKinds": [
                "quickfix",
                "refactor",
                "refactor.extract",
                "refactor.inline",
                "refactor.rewrite",
                "source",
                "source.organizeImports"
            ]
        },
        "codeLensProvider": {
            "resolveProvider": true
        },
        "documentFormattingProvider": true,
        "documentRangeFormattingProvider": true,
        "documentOnTypeFormattingProvider": {
            "firstTriggerCharacter": "}",
            "moreTriggerCharacter": [";", "\n"]
        },
        "renameProvider": {
            "prepareProvider": true
        },
        "foldingRangeProvider": true,
        "selectionRangeProvider": true,
        "semanticTokensProvider": {
            "legend": {
                "tokenTypes": [
                    "class", "parameter", "variable", "function", "keyword", "string", "number", "operator"
                ],
                "tokenModifiers": []
            },
            "range": true,
            "full": true
        },
        "inlayHintProvider": true,
        "diagnosticProvider": {
            "identifier": "valkyrie-lsp",
            "interFileDependencies": true,
            "workspaceDiagnostics": true
        },
        "workspace": {
            "workspaceFolders": {
                "supported": true,
                "changeNotifications": true
            },
            "fileOperations": {
                "didCreate": {
                    "filters": [{
                        "scheme": "file",
                        "pattern": { "glob": "**/*.{vk,awsl}", "matches": "file" }
                    }]
                },
                "didRename": {
                    "filters": [{
                        "scheme": "file",
                        "pattern": { "glob": "**/*.{vk,awsl}", "matches": "file" }
                    }]
                },
                "didDelete": {
                    "filters": [{
                        "scheme": "file",
                        "pattern": { "glob": "**/*.{vk,awsl}", "matches": "file" }
                    }]
                }
            }
        },
        "callHierarchyProvider": true,
        "linkedEditingRangeProvider": true,
        "monikerProvider": true
    })
}
