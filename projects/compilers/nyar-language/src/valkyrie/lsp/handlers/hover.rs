use crate::state::{ClassInfo, ServerState, SymbolInfo, TypeSignature};
use crate::types::Position;
use crate::handlers;
use oak_lsp::types::Hover;

/// Hover 信息处理器
pub struct HoverHandler;

impl HoverHandler {
    /// 处理 Hover 请求
    pub async fn handle(state: &ServerState, uri: &str, position: Position) -> Option<Hover> {
        if handlers::awsl::document::is_awsl_uri(uri) {
            if let Some(hover) = handlers::awsl::AwslHoverHandler::handle(state, uri, position).await {
                return Some(hover);
            }
        }

        let symbol = state.query_symbol_at_position(uri, position).await?;
        let markdown = Self::format_symbol_hover(&symbol);

        Some(Hover { contents: markdown, range: Some(symbol.location.range) })
    }

    /// 格式化符号的 Hover 信息为 Markdown
    pub(crate) fn format_symbol_hover(symbol: &SymbolInfo) -> String {
        let mut markdown = String::new();

        Self::format_header(&mut markdown, symbol);

        if let Some(ref signature) = symbol.signature {
            Self::format_signature(&mut markdown, signature);
        }

        if let Some(ref class_info) = symbol.class_info {
            Self::format_class_info(&mut markdown, class_info);
        }

        if let Some(ref doc) = symbol.documentation {
            Self::format_documentation(&mut markdown, doc);
        }

        markdown
    }

    /// 格式化标题部分
    fn format_header(markdown: &mut String, symbol: &SymbolInfo) {
        let kind_icon = Self::get_kind_icon(&symbol.kind);
        markdown.push_str(&format!("### {} {}\n\n", kind_icon, symbol.name));

        if let Some(ref type_info) = symbol.type_info {
            markdown.push_str(&format!("```valkyrie\n{}\n```\n\n", type_info));
        }

        if !symbol.namespace.is_empty() {
            markdown.push_str(&format!("*namespace:* `{}`\n\n", symbol.namespace));
        }
    }

    /// 格式化函数签名详情
    fn format_signature(markdown: &mut String, signature: &TypeSignature) {
        if !signature.type_parameters.is_empty() {
            markdown.push_str("**Type Parameters:**\n\n");
            for tp in &signature.type_parameters {
                markdown.push_str(&format!("- `{}`\n", tp));
            }
            markdown.push('\n');
        }

        if !signature.parameters.is_empty() {
            markdown.push_str("**Parameters:**\n\n");
            for param in &signature.parameters {
                let mut param_line = format!("- `{}`", param.name);
                if let Some(ref ty) = param.ty {
                    param_line.push_str(&format!(": `{}`", ty));
                }
                if param.is_optional {
                    param_line.push_str(" *(optional)*");
                }
                if param.is_variadic {
                    param_line.push_str(" *(variadic)*");
                }
                markdown.push_str(&param_line);
                markdown.push('\n');
            }
            markdown.push('\n');
        }

        if let Some(ref return_type) = signature.return_type {
            markdown.push_str(&format!("**Returns:** `{}`\n\n", return_type));
        }
    }

    /// 格式化类信息
    fn format_class_info(markdown: &mut String, class_info: &ClassInfo) {
        if let Some(ref parent) = class_info.parent_class {
            markdown.push_str(&format!("**Extends:** `{}`\n\n", parent));
        }

        if !class_info.implements.is_empty() {
            markdown.push_str("**Implements:**\n\n");
            for impl_name in &class_info.implements {
                markdown.push_str(&format!("- `{}`\n", impl_name));
            }
            markdown.push('\n');
        }

        if !class_info.members.is_empty() {
            markdown.push_str("**Members:**\n\n");
            let method_count = class_info.members.iter().filter(|m| m.kind == "method").count();
            let func_count = class_info.members.iter().filter(|m| m.kind == "function").count();

            if method_count > 0 {
                markdown.push_str(&format!("- {} method(s)\n", method_count));
            }
            if func_count > 0 {
                markdown.push_str(&format!("- {} function(s)\n", func_count));
            }

            markdown.push_str("\n<details>\n<summary>View all members</summary>\n\n");
            for member in &class_info.members {
                let icon = Self::get_kind_icon(&member.kind);
                if let Some(ref type_info) = member.type_info {
                    markdown.push_str(&format!("{} `{}`: `{}`\n", icon, member.name, type_info));
                } else {
                    markdown.push_str(&format!("{} `{}`\n", icon, member.name));
                }
            }
            markdown.push_str("\n</details>\n\n");
        }
    }

    /// 格式化文档注释
    fn format_documentation(markdown: &mut String, doc: &str) {
        markdown.push_str("---\n\n");
        markdown.push_str(doc);
        markdown.push('\n');
    }

    /// 获取符号类型对应的图标
    fn get_kind_icon(kind: &str) -> &'static str {
        match kind.to_lowercase().as_str() {
            "function" => "ƒ",
            "method" => "⚡",
            "class" => "📦",
            "interface" => "🔌",
            "variable" => "📝",
            "constant" => "🔒",
            "property" => "🏷️",
            "field" => "📌",
            "enum" => "📋",
            "struct" => "🏗️",
            "module" => "📁",
            "namespace" => "📚",
            "member" => "🔹",
            _ => "•",
        }
    }
}
