//! AWSL 补全

use super::script::resolve_script_view;
use crate::handlers::completion::CompletionHandler;
use crate::{state::ServerState, types::Position};
use oak_lsp::types::{CompletionItem, CompletionItemKind};

const DIRECTIVES: &[&str] = &["@if", "@for", "@bind", "@ref", "@class", "@click", "@on:click"];
const CONTROL_TAGS: &[&str] = &["widget", "template", "script", "style", "if", "for", "loop"];
const HTML_TAGS: &[&str] = &[
    "div", "span", "p", "a", "button", "input", "section", "article", "h1", "h2", "h3", "ul", "ol", "li", "flex", "text",
];

pub struct AwslCompletionHandler;

impl AwslCompletionHandler {
    pub async fn handle(state: &ServerState, uri: &str, position: Position) -> Vec<CompletionItem> {
        let doc = match state.documents.get(uri) {
            Some(d) => d.clone(),
            None => return Vec::new(),
        };

        // script 块内委托 Valkyrie 补全
        if let Some(view) = resolve_script_view(&doc, position) {
            return CompletionHandler::complete_at(state, &view.script_doc, &view.ast, view.script_position)
                .await
                .unwrap_or_default();
        }

        let root = match &doc.awsl_root {
            Some(r) => r.clone(),
            None => return Vec::new(),
        };

        let offset = doc.position_to_offset(position);
        let text_before = if offset > 0 { &doc.text[..offset] } else { "" };
        let mut items = Vec::new();

        if let Some(binding) = std_data::text::awsl::find_template_binding_at(&root, offset) {
            if let Some(entry) = state.awsl_abi_for_widget(&std_data::text::awsl::awsl_stem_from_component_tag(
                &binding.component_tag,
            )) {
                let names: Vec<&str> = match binding.kind {
                    std_data::text::awsl::TemplateBindingKind::Property => {
                        entry.abi.properties.iter().map(|p| p.name.as_str()).collect()
                    }
                    std_data::text::awsl::TemplateBindingKind::Event => {
                        entry.abi.events.iter().map(|e| e.name.as_str()).collect()
                    }
                };
                for name in names {
                    items.push(CompletionItem {
                        label: format!(
                            "{}{}",
                            if matches!(binding.kind, std_data::text::awsl::TemplateBindingKind::Property) {
                                ":"
                            } else {
                                "@"
                            },
                            name
                        ),
                        kind: Some(CompletionItemKind::Property),
                        ..Default::default()
                    });
                }
                return items;
            }
        }

        if text_before.ends_with('@') || text_before.contains("@") && !text_before.ends_with('>') {
            for directive in DIRECTIVES {
                items.push(CompletionItem {
                    label: directive.to_string(),
                    kind: Some(CompletionItemKind::Property),
                    ..Default::default()
                });
            }
        }

        if text_before.ends_with('<') || text_before.ends_with("</") {
            for tag in CONTROL_TAGS.iter().chain(HTML_TAGS.iter()) {
                items.push(CompletionItem {
                    label: tag.to_string(),
                    kind: Some(CompletionItemKind::Class),
                    ..Default::default()
                });
            }
            for import in &root.imports {
                items.push(CompletionItem {
                    label: import.name.clone(),
                    kind: Some(CompletionItemKind::Module),
                    detail: Some(import.from.clone()),
                    ..Default::default()
                });
            }
        }

        items
    }
}
