//! AWSL ABI cross-file references and rename locations.

use std::ops::Range;

use oak_lsp::types::LocationRange;
use std_data::text::awsl::{
    abi_declaration_span, awsl_stem_from_component_tag, classify_abi_cursor, collect_abi_references,
    collect_template_bindings, find_template_binding_at, AbiSymbolKind, ComponentAbiIndex, TemplateBindingKind,
};

use super::document::{is_awsl_uri, map_script_range_to_file, script_offset_range};
use super::widget::map_synthetic_span_to_file;
use super::widget::{component_stem_from_uri, widget_name_from_stem};
use crate::state::{AwslAbiEntry, ServerState};
use crate::types::Position;

pub struct AwslReferencesHandler;

impl AwslReferencesHandler {
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

        if let Some(binding) = find_template_binding_at(&root, offset) {
            return Self::references_for_template_binding(state, &binding).await;
        }

        if let Some(abi) = &doc.component_abi {
            if let Some((kind, name, _)) = classify_abi_cursor(abi, &root, offset) {
                return Self::references_for_abi_symbol(state, uri, abi, kind, &name).await;
            }
        }

        if let Some(emit_name) = Self::emit_target_at(state, uri, position).await {
            return Self::references_for_emit(state, uri, &emit_name).await;
        }

        vec![]
    }

    pub async fn rename_locations(state: &ServerState, uri: &str, position: Position) -> Vec<(String, Range<usize>)> {
        Self::handle(state, uri, position)
            .await
            .into_iter()
            .map(|loc| (loc.uri.to_string(), loc.range))
            .collect()
    }

    async fn references_for_template_binding(
        state: &ServerState,
        binding: &std_data::text::awsl::TemplateBinding,
    ) -> Vec<LocationRange> {
        let widget = awsl_stem_from_component_tag(&binding.component_tag);
        let Some(entry) = state.awsl_abi_for_widget(&widget) else {
            return Vec::new();
        };
        let decl_kind = match binding.kind {
            TemplateBindingKind::Property => AbiSymbolKind::Property,
            TemplateBindingKind::Event => AbiSymbolKind::Event,
        };
        let mut locations = vec![Self::abi_decl_location(&entry, decl_kind, &binding.name)];
        locations.extend(Self::template_binding_locations(state, &binding.name, binding.kind));
        locations.extend(Self::script_binding_locations(state, &binding.name));
        locations.into_iter().filter(|l| !l.range.is_empty()).collect()
    }

    async fn references_for_abi_symbol(
        state: &ServerState,
        owner_uri: &str,
        abi: &std_data::text::awsl::ComponentAbi,
        kind: AbiSymbolKind,
        name: &str,
    ) -> Vec<LocationRange> {
        let mut locations = Vec::new();
        if let Some(entry) = state.awsl_abi_index.iter().find(|e| e.uri == owner_uri) {
            locations.push(Self::abi_decl_location(&entry, kind, name));
        }
        locations.extend(Self::template_binding_locations(state, name, Self::template_kind(kind)));
        locations.extend(Self::emit_locations(state, name));
        locations.extend(Self::script_binding_locations(state, name));
        if let Some(owner) = state.documents.get(owner_uri) {
            if let Some(root) = &owner.awsl_root {
                for reference in collect_abi_references(abi, owner.script_text().as_deref().unwrap_or(""), root) {
                    if reference.name == name {
                        locations.push(LocationRange {
                            uri: owner_uri.to_string().into(),
                            range: reference.span,
                        });
                    }
                }
            }
        }
        locations
    }

    async fn references_for_emit(state: &ServerState, owner_uri: &str, event_name: &str) -> Vec<LocationRange> {
        let mut locations = Vec::new();
        if let Some(entry) = state.awsl_abi_for_widget(&Self::widget_for_uri(state, owner_uri)) {
            locations.push(Self::abi_decl_location(&entry, AbiSymbolKind::Event, event_name));
        }
        locations.extend(Self::template_binding_locations(state, event_name, TemplateBindingKind::Event));
        locations.extend(Self::emit_locations(state, event_name));
        locations
    }

    fn abi_decl_location(entry: &AwslAbiEntry, kind: AbiSymbolKind, name: &str) -> LocationRange {
        let span = abi_declaration_span(&entry.abi, kind, name).unwrap_or(0..0);
        let range = if span.is_empty() {
            0..0
        }
        else {
            span.start + entry.script_base_offset..span.end + entry.script_base_offset
        };
        LocationRange {
            uri: entry.uri.clone().into(),
            range,
        }
    }

    fn template_binding_locations(
        state: &ServerState,
        name: &str,
        kind: TemplateBindingKind,
    ) -> Vec<LocationRange> {
        let mut out = Vec::new();
        for doc_ref in state.documents.iter() {
            if !is_awsl_uri(doc_ref.key()) {
                continue;
            }
            let Some(root) = &doc_ref.awsl_root else {
                continue;
            };
            for binding in collect_template_bindings(root) {
                if binding.name == name && binding.kind == kind {
                    out.push(LocationRange {
                        uri: doc_ref.key().clone().into(),
                        range: binding.key_span.clone(),
                    });
                }
            }
        }
        out
    }

    fn script_binding_locations(state: &ServerState, name: &str) -> Vec<LocationRange> {
        let mut out = Vec::new();
        for doc_ref in state.documents.iter() {
            if !is_awsl_uri(doc_ref.key()) {
                continue;
            }
            let Some(ast) = &doc_ref.ast else {
                continue;
            };
            let Some((base, prefix)) = (doc_ref.awsl_script_base, doc_ref.awsl_synthetic_prefix) else {
                continue;
            };
            Self::collect_ident_refs_in_items(&ast.items, name, doc_ref.key(), base, prefix, &mut out);
        }
        out
    }

    fn emit_locations(state: &ServerState, event_name: &str) -> Vec<LocationRange> {
        let mut out = Vec::new();
        for doc_ref in state.documents.iter() {
            if !is_awsl_uri(doc_ref.key()) {
                continue;
            }
            let Some(ast) = &doc_ref.ast else {
                continue;
            };
            let Some((base, prefix)) = (doc_ref.awsl_script_base, doc_ref.awsl_synthetic_prefix) else {
                continue;
            };
            Self::collect_emit_refs_in_items(&ast.items, event_name, doc_ref.key(), base, prefix, &mut out);
        }
        out
    }

    async fn emit_target_at(state: &ServerState, uri: &str, position: Position) -> Option<String> {
        use super::script::resolve_script_view;
        let doc = state.documents.get(uri)?.clone();
        let view = resolve_script_view(&doc, position)?;
        let script_offset = view.script_doc.position_to_offset(view.script_position);
        let first_arg = Self::emit_first_arg_at(&view.ast.items, script_offset)?;
        Some(first_arg)
    }

    fn widget_for_uri(state: &ServerState, uri: &str) -> String {
        if let Some(doc) = state.documents.get(uri) {
            if let Some(root) = &doc.awsl_root {
                return widget_name_from_stem(&component_stem_from_uri(uri));
            }
        }
        component_stem_from_uri(uri)
    }

    fn template_kind(kind: AbiSymbolKind) -> TemplateBindingKind {
        match kind {
            AbiSymbolKind::Event | AbiSymbolKind::EmitTarget => TemplateBindingKind::Event,
            _ => TemplateBindingKind::Property,
        }
    }

    fn collect_ident_refs_in_items(
        items: &[oak_valkyrie::ast::Item],
        name: &str,
        uri: &str,
        base: usize,
        prefix: usize,
        out: &mut Vec<LocationRange>,
    ) {
        for item in items {
            match item {
                oak_valkyrie::ast::Item::Widget(w) => {
                    Self::collect_ident_refs_in_items(&w.items, name, uri, base, prefix, out);
                }
                oak_valkyrie::ast::Item::Statement(stmt) => {
                    Self::collect_ident_refs_in_stmt(stmt, name, uri, base, prefix, out);
                }
                _ => {}
            }
        }
    }

    fn collect_ident_refs_in_stmt(
        stmt: &oak_valkyrie::ast::Statement,
        name: &str,
        uri: &str,
        base: usize,
        prefix: usize,
        out: &mut Vec<LocationRange>,
    ) {
        use oak_valkyrie::ast::{Expr, Pattern, Statement};
        match stmt {
            Statement::Let { pattern, expr, .. } => {
                if let Pattern::Variable { name: id, .. } = pattern {
                    if id.name == name {
                        out.push(LocationRange {
                            uri: uri.to_string().into(),
                            range: map_synthetic_span_to_file(base, prefix, id.span.clone()),
                        });
                    }
                }
                Self::collect_ident_refs_in_expr(expr, name, uri, base, prefix, out);
            }
            Statement::ExprStmt { expr, .. } => {
                Self::collect_ident_refs_in_expr(expr, name, uri, base, prefix, out);
            }
        }
    }

    fn collect_ident_refs_in_expr(
        expr: &oak_valkyrie::ast::Expr,
        name: &str,
        uri: &str,
        base: usize,
        prefix: usize,
        out: &mut Vec<LocationRange>,
    ) {
        use oak_valkyrie::ast::Expr;
        match expr {
            Expr::Ident(id) if id.name == name => {
                out.push(LocationRange {
                    uri: uri.to_string().into(),
                    range: map_synthetic_span_to_file(base, prefix, id.span.clone()),
                });
            }
            Expr::Call { callee, args, .. } => {
                Self::collect_ident_refs_in_expr(callee, name, uri, base, prefix, out);
                for arg in args {
                    Self::collect_ident_refs_in_expr(arg, name, uri, base, prefix, out);
                }
            }
            Expr::Binary { left, right, .. } => {
                Self::collect_ident_refs_in_expr(left, name, uri, base, prefix, out);
                Self::collect_ident_refs_in_expr(right, name, uri, base, prefix, out);
            }
            _ => {}
        }
    }

    fn collect_emit_refs_in_items(
        items: &[oak_valkyrie::ast::Item],
        event_name: &str,
        uri: &str,
        base: usize,
        prefix: usize,
        out: &mut Vec<LocationRange>,
    ) {
        for item in items {
            if let oak_valkyrie::ast::Item::Widget(w) = item {
                Self::collect_emit_refs_in_items(&w.items, event_name, uri, base, prefix, out);
            }
            if let oak_valkyrie::ast::Item::Statement(stmt) = item {
                Self::collect_emit_refs_in_stmt(stmt, event_name, uri, base, prefix, out);
            }
        }
    }

    fn collect_emit_refs_in_stmt(
        stmt: &oak_valkyrie::ast::Statement,
        event_name: &str,
        uri: &str,
        base: usize,
        prefix: usize,
        out: &mut Vec<LocationRange>,
    ) {
        use oak_valkyrie::ast::{Expr, Statement};
        match stmt {
            Statement::ExprStmt { expr, .. } => Self::collect_emit_refs_in_expr(expr, event_name, uri, base, prefix, out),
            Statement::Let { expr, .. } => Self::collect_emit_refs_in_expr(expr, event_name, uri, base, prefix, out),
        }
    }

    fn collect_emit_refs_in_expr(
        expr: &oak_valkyrie::ast::Expr,
        event_name: &str,
        uri: &str,
        base: usize,
        prefix: usize,
        out: &mut Vec<LocationRange>,
    ) {
        use oak_valkyrie::ast::Expr;
        if let Expr::Call { callee, args, span } = expr {
            if let Expr::Ident(id) = callee.as_ref() {
                if id.name == "emit" {
                    if let Some(Expr::Ident(arg)) = args.first() {
                        if arg.name == event_name {
                            out.push(LocationRange {
                                uri: uri.to_string().into(),
                                range: map_synthetic_span_to_file(base, prefix, arg.span.clone()),
                            });
                            return;
                        }
                    }
                }
            }
            Self::collect_emit_refs_in_expr(callee, event_name, uri, base, prefix, out);
            for arg in args {
                Self::collect_emit_refs_in_expr(arg, event_name, uri, base, prefix, out);
            }
        }
    }

    fn emit_first_arg_at(items: &[oak_valkyrie::ast::Item], offset: usize) -> Option<String> {
        for item in items {
            if let oak_valkyrie::ast::Item::Widget(w) = item {
                if let Some(name) = Self::emit_first_arg_in_items(&w.items, offset) {
                    return Some(name);
                }
            }
        }
        None
    }

    fn emit_first_arg_in_items(items: &[oak_valkyrie::ast::Item], offset: usize) -> Option<String> {
        for item in items {
            match item {
                oak_valkyrie::ast::Item::Statement(stmt) => {
                    if let Some(name) = Self::emit_first_arg_in_stmt(stmt, offset) {
                        return Some(name);
                    }
                }
                oak_valkyrie::ast::Item::Widget(w) => {
                    if let Some(name) = Self::emit_first_arg_in_items(&w.items, offset) {
                        return Some(name);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn emit_first_arg_in_stmt(stmt: &oak_valkyrie::ast::Statement, offset: usize) -> Option<String> {
        use oak_valkyrie::ast::{Expr, Statement};
        match stmt {
            Statement::ExprStmt { expr, .. } => Self::emit_first_arg_in_expr(expr, offset),
            Statement::Let { expr, .. } => Self::emit_first_arg_in_expr(expr, offset),
        }
    }

    fn emit_first_arg_in_expr(expr: &oak_valkyrie::ast::Expr, offset: usize) -> Option<String> {
        use oak_valkyrie::ast::Expr;
        if let Expr::Call { callee, args, .. } = expr {
            if let Expr::Ident(id) = callee.as_ref() {
                if id.name == "emit" {
                    if let Some(Expr::Ident(arg)) = args.first() {
                        if arg.span.contains(&offset) {
                            return Some(arg.name.clone());
                        }
                    }
                }
            }
        }
        None
    }
}

trait AwslDocumentScript {
    fn script_text(&self) -> Option<String>;
}

impl AwslDocumentScript for crate::state::DocumentState {
    fn script_text(&self) -> Option<String> {
        self.awsl_root.as_ref().and_then(|r| r.script.clone())
    }
}
