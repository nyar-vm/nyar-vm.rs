//! AWSL 定义跳转

use crate::{state::ServerState, types::Position};
use oak_lsp::types::LocationRange;
use std::path::{Path, PathBuf};
use std_data::text::awsl::{
    abi_declaration_span, awsl_stem_from_component_tag, classify_abi_cursor, find_template_binding_at, AbiSymbolKind,
    TemplateBindingKind,
};
use url::Url;

pub struct AwslDefinitionHandler;

impl AwslDefinitionHandler {
    pub async fn handle(state: &ServerState, uri: &str, position: Position) -> Vec<LocationRange> {
        let doc = match state.documents.get(uri) {
            Some(d) => d.clone(),
            None => return Vec::new(),
        };

        let root = match &doc.awsl_root {
            Some(r) => r.clone(),
            None => return Vec::new(),
        };

        let offset = doc.position_to_offset(position);
        let text_at = doc.text.get(offset.saturating_sub(20)..offset + 20).unwrap_or("");

        // `:prop` / `@event` on PascalCase child components → target ABI declaration
        if let Some(binding) = find_template_binding_at(&root, offset) {
            let widget = awsl_stem_from_component_tag(&binding.component_tag);
            if let Some(entry) = state.awsl_abi_for_widget(&widget) {
                let kind = match binding.kind {
                    TemplateBindingKind::Property => AbiSymbolKind::Property,
                    TemplateBindingKind::Event => AbiSymbolKind::Event,
                };
                if let Some(span) = abi_declaration_span(&entry.abi, kind, &binding.name) {
                    return vec![LocationRange {
                        uri: entry.uri.clone().into(),
                        range: span.start + entry.script_base_offset..span.end + entry.script_base_offset,
                    }];
                }
            }
        }

        // ABI declaration in script → stay on definition
        if let Some(abi) = &doc.component_abi {
            if let Some((kind, name, _)) = classify_abi_cursor(abi, &root, offset) {
                if let Some(span) = abi_declaration_span(abi, kind, &name) {
                    if let Some(base) = doc.awsl_script_base {
                        return vec![LocationRange {
                            uri: uri.to_string().into(),
                            range: span.start + base..span.end + base,
                        }];
                    }
                }
            }
        }

        // import from="path" 跳转
        for import in &root.imports {
            if import.span.contains(&offset) || text_at.contains(&import.from) {
                if let Some(target) = resolve_import_path(uri, &import.from) {
                    if let Ok(target_uri) = Url::from_file_path(&target) {
                        return vec![LocationRange {
                            uri: target_uri.to_string().into(),
                            range: 0..1,
                        }];
                    }
                }
            }
        }

        // PascalCase 组件标签跳转（AWSL 组件 ↔ Valkyrie widget 统一索引）
        if let Some(tag) = find_component_tag_at(&doc.text, offset) {
            if tag.chars().next().is_some_and(|c| c.is_uppercase()) {
                if let Some(loc) = find_widget_in_workspace(state, &tag) {
                    return vec![loc];
                }
                if let Some(target) = find_component_file(uri, &tag) {
                    if let Ok(target_uri) = Url::from_file_path(&target) {
                        return vec![LocationRange {
                            uri: target_uri.to_string().into(),
                            range: 0..1,
                        }];
                    }
                }
            }
        }

        // script 块内委托 Valkyrie 定义跳转（偏移映射回源文件）
        if let Some(symbol) = state.query_awsl_script_symbol_at_position(uri, position).await {
            return vec![symbol.location];
        }

        vec![]
    }
}

fn resolve_import_path(base_uri: &str, import_path: &str) -> Option<PathBuf> {
    let base_url = Url::parse(base_uri).ok()?;
    let base_path = base_url.to_file_path().ok()?;
    let base_dir = base_path.parent()?;
    let clean = import_path.trim_matches('"').trim_matches('\'');
    let resolved = base_dir.join(clean);
    if resolved.exists() {
        Some(resolved)
    }
    else {
        None
    }
}

fn find_component_file(base_uri: &str, component_name: &str) -> Option<PathBuf> {
    let base_url = Url::parse(base_uri).ok()?;
    let base_path = base_url.to_file_path().ok()?;
    let base_dir = base_path.parent()?;
    let stem = awsl_stem_from_component_tag(component_name);
    for candidate_stem in [stem.as_str(), component_name, &component_name.to_lowercase()] {
        let candidate = base_dir.join(format!("{candidate_stem}.awsl"));
        if candidate.exists() {
            return Some(candidate);
        }
        let kebab = candidate_stem.replace('_', "-");
        let kebab_path = base_dir.join(format!("{kebab}.awsl"));
        if kebab_path.exists() {
            return Some(kebab_path);
        }
    }
    find_awsl_in_dir(base_dir, component_name)
        .or_else(|| find_valkyrie_widget_file(base_dir, component_name))
}

fn find_valkyrie_widget_file(dir: &Path, component_name: &str) -> Option<PathBuf> {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if matches!(ext, "v" | "valkyrie" | "vx") {
                    if let Ok(text) = std::fs::read_to_string(&path) {
                        if text_contains_widget(&text, component_name) {
                            return Some(path);
                        }
                    }
                }
            }
        }
    }
    None
}

fn text_contains_widget(source: &str, name: &str) -> bool {
    source.contains(&format!("widget {name}"))
        || source.contains(&format!("widget {name} {{"))
        || source.contains(&format!("widget {name}("))
}

fn find_widget_in_workspace(state: &ServerState, component_name: &str) -> Option<LocationRange> {
    use oak_lsp::types::SymbolKind;
    for ns_entry in state.index.symbols.iter() {
        for sym in ns_entry.value().iter() {
            if matches!(sym.kind, SymbolKind::Class | SymbolKind::Struct)
                && sym.name.eq_ignore_ascii_case(component_name)
            {
                return Some(LocationRange {
                    uri: sym.uri.clone().into(),
                    range: sym.range.clone(),
                });
            }
        }
    }
    None
}

fn find_awsl_in_dir(dir: &Path, component_name: &str) -> Option<PathBuf> {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "awsl") {
                if path.file_stem().is_some_and(|s| s.eq_ignore_ascii_case(component_name)) {
                    return Some(path);
                }
            }
        }
    }
    None
}

fn find_component_tag_at(source: &str, offset: usize) -> Option<String> {
    let before = &source[..offset.min(source.len())];
    let open = before.rfind('<')?;
    let fragment = &before[open + 1..];
    if fragment.starts_with('/') {
        return None;
    }
    let name: String = fragment.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    if name.is_empty() { None } else { Some(name) }
}
