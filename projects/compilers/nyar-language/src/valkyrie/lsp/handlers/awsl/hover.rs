//! AWSL Hover 支持

use crate::handlers::HoverHandler;
use crate::{state::ServerState, types::Position};
use oak_lsp::types::Hover;

const DIRECTIVE_DOCS: &[(&str, &str)] = &[
    ("@if", "条件渲染指令"),
    ("@for", "列表渲染指令"),
    ("@bind", "双向绑定指令"),
    ("@ref", "DOM 引用指令"),
    ("@class", "类名合并指令"),
    ("@click", "点击事件绑定"),
    ("@on:click", "点击事件绑定（完整形式）"),
];

pub struct AwslHoverHandler;

impl AwslHoverHandler {
    pub async fn handle(state: &ServerState, uri: &str, position: Position) -> Option<Hover> {
        let doc = state.documents.get(uri)?;
        let offset = doc.position_to_offset(position);
        let text = &doc.text;
        let context = text.get(offset.saturating_sub(30)..offset + 30).unwrap_or("");

        for (directive, doc_text) in DIRECTIVE_DOCS {
            if context.contains(directive) {
                return Some(Hover {
                    contents: format!("**{directive}**\n\n{doc_text}"),
                    range: Some(offset..offset + directive.len()),
                });
            }
        }

        if context.contains("import:") || context.contains("from=") {
            return Some(Hover {
                contents: "**AWSL Import**\n\n导入 AWSL 组件：`<import:Name from=\"path.awsl\"/>`".to_string(),
                range: None,
            });
        }

        if let Some(root) = &doc.awsl_root {
            if let Some(widget_name) = &root.widget_name {
                if context.contains('<') {
                    return Some(Hover {
                        contents: format!("**Widget `{widget_name}`**\n\nAWSL 组件定义根节点"),
                        range: None,
                    });
                }
            }
        }

        // script 块内委托 Valkyrie 符号查询（偏移映射回源文件）
        if let Some(symbol) = state.query_awsl_script_symbol_at_position(uri, position).await {
            return Some(Hover {
                contents: HoverHandler::format_symbol_hover(&symbol),
                range: Some(symbol.location.range),
            });
        }

        None
    }
}
