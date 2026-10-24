//! Surface → canonical RenderIR lowering.

use std::collections::HashMap;

use super::{
    canonical::{
        RenderAttr, RenderAttrValue, RenderComponentNode, RenderElement, RenderFragment, RenderIfNode, RenderLoopNode, RenderModule,
        RenderNode, RenderRegion, RenderText, RenderTextSegment,
    },
    expr::{RenderBinding, RenderEvent, RenderExpr, RenderExprKind, RenderPurity, RenderValueKind},
    ids::{RenderBindingId, RenderEventId, RenderExprId, RenderNodeId, RenderRegionId},
    surface::{SurfaceAttr, SurfaceAttrValue, SurfaceIf, SurfaceIr, SurfaceLoop, SurfaceNode, SurfaceTextPart, TemplateNodeKind},
};
use crate::awsl::{ScriptBinding, SignalValueType, lower::BindingKind};

struct Canonicalizer {
    module: RenderModule,
    script_bindings: Vec<ScriptBinding>,
    binding_name_to_id: HashMap<String, RenderBindingId>,
    expr_intern: HashMap<String, RenderExprId>,
}

impl Canonicalizer {
    fn new(script_bindings: &[ScriptBinding]) -> Self {
        let mut module = RenderModule::empty();
        let mut binding_name_to_id = HashMap::new();
        for binding in script_bindings {
            let init_expr = module.exprs.len() as u32;
            let expr_id = RenderExprId(init_expr);
            module.exprs.push(build_expr_record(&binding.init_expr, RenderExprKind::BindingInit, script_bindings, &binding_name_to_id));
            let binding_id = RenderBindingId(module.bindings.len() as u32);
            module.bindings.push(RenderBinding {
                name: binding.name.clone(),
                init_expr: expr_id,
                reactive: binding.reactive,
                sig_var: if binding.reactive { binding.sig_var.clone() } else { String::new() },
            });
            binding_name_to_id.insert(binding.name.clone(), binding_id);
        }
        Self { module, script_bindings: script_bindings.to_vec(), binding_name_to_id, expr_intern: HashMap::new() }
    }

    fn finish(mut self, surface: SurfaceIr) -> RenderModule {
        self.module.roots = self.lower_nodes(surface);
        self.module
    }

    fn lower_nodes(&mut self, nodes: SurfaceIr) -> Vec<RenderNodeId> {
        nodes.into_iter().filter_map(|node| self.lower_node(node)).collect()
    }

    fn lower_node(&mut self, node: SurfaceNode) -> Option<RenderNodeId> {
        match node {
            SurfaceNode::Tag { tag, kind, attrs, children, span } => {
                let lowered_attrs = attrs.into_iter().map(|attr| self.lower_attr(attr)).collect();
                let child_nodes = self.lower_nodes(children);
                let child_region = self.alloc_region(child_nodes);
                let canonical = match kind {
                    TemplateNodeKind::Component => {
                        RenderNode::Component(RenderComponentNode { tag, attrs: lowered_attrs, children: child_region, span })
                    }
                    TemplateNodeKind::Intrinsic | TemplateNodeKind::HostView => {
                        RenderNode::Element(RenderElement { tag, kind, attrs: lowered_attrs, children: child_region, span })
                    }
                };
                let id = RenderNodeId(self.module.nodes.len() as u32);
                self.module.nodes.push(canonical);
                Some(id)
            }
            SurfaceNode::Text { parts, span } => {
                let segments = self.lower_text_parts(parts);
                let id = RenderNodeId(self.module.nodes.len() as u32);
                self.module.nodes.push(RenderNode::Text(RenderText { segments, span }));
                Some(id)
            }
            SurfaceNode::If(surface_if) => {
                let canonical_if = self.lower_if(surface_if);
                let id = RenderNodeId(self.module.nodes.len() as u32);
                self.module.nodes.push(RenderNode::If(canonical_if));
                Some(id)
            }
            SurfaceNode::Loop(surface_loop) => {
                let canonical_loop = self.lower_loop(surface_loop);
                let id = RenderNodeId(self.module.nodes.len() as u32);
                self.module.nodes.push(RenderNode::Loop(canonical_loop));
                Some(id)
            }
            SurfaceNode::Fragment { children, span } => {
                let child_nodes = self.lower_nodes(children);
                let child_region = self.alloc_region(child_nodes);
                let id = RenderNodeId(self.module.nodes.len() as u32);
                self.module.nodes.push(RenderNode::Fragment(RenderFragment { children: child_region, span }));
                Some(id)
            }
        }
    }

    fn lower_if(&mut self, surface_if: SurfaceIf) -> RenderIfNode {
        let then_nodes = self.lower_nodes(surface_if.then_branch);
        let else_nodes = self.lower_nodes(surface_if.else_branch);
        RenderIfNode {
            condition: self.intern_template_expr(&surface_if.condition),
            then_region: self.alloc_region(then_nodes),
            else_region: self.alloc_region(else_nodes),
            span: surface_if.span,
        }
    }

    fn lower_loop(&mut self, surface_loop: SurfaceLoop) -> RenderLoopNode {
        let item_binding = self.ensure_loop_binding(&surface_loop.item_var);
        let index_binding = if surface_loop.index_var.is_empty() { None } else { Some(self.ensure_loop_binding(&surface_loop.index_var)) };
        let body_nodes = self.lower_nodes(surface_loop.body);
        RenderLoopNode {
            items: self.intern_template_expr(&surface_loop.items_expr),
            key: surface_loop.key_expr.as_deref().map(|expr| self.intern_template_expr(expr)),
            item_binding,
            index_binding,
            item_var: surface_loop.item_var,
            index_var: surface_loop.index_var,
            body_region: self.alloc_region(body_nodes),
            span: surface_loop.span,
        }
    }

    fn ensure_loop_binding(&mut self, name: &str) -> RenderBindingId {
        if let Some(id) = self.binding_name_to_id.get(name).copied() {
            return id;
        }
        let expr_id = self.intern_template_expr(name);
        let binding_id = RenderBindingId(self.module.bindings.len() as u32);
        self.module.bindings.push(RenderBinding { name: name.to_string(), init_expr: expr_id, reactive: false, sig_var: String::new() });
        self.binding_name_to_id.insert(name.to_string(), binding_id);
        binding_id
    }

    fn lower_attr(&mut self, attr: SurfaceAttr) -> RenderAttr {
        let (value, event_id) = match attr.value {
            SurfaceAttrValue::Static(text) => (RenderAttrValue::Static(text), None),
            SurfaceAttrValue::Dynamic(expr) => {
                let expr_id = self.intern_template_expr(&expr);
                let event_id = if attr.is_event { Some(self.register_event(&attr.name, expr_id)) } else { None };
                (RenderAttrValue::Expr(expr_id), event_id)
            }
            SurfaceAttrValue::Mixed(parts) => (RenderAttrValue::Template(self.lower_text_parts(parts)), None),
        };
        RenderAttr { name: attr.name, value, is_event: attr.is_event, is_prop: attr.is_prop, event_id }
    }

    fn register_event(&mut self, name: &str, handler: RenderExprId) -> RenderEventId {
        let id = RenderEventId(self.module.events.len() as u32);
        self.module.events.push(RenderEvent { name: name.to_string(), handler_expr: handler });
        id
    }

    fn lower_text_parts(&mut self, parts: Vec<SurfaceTextPart>) -> Vec<RenderTextSegment> {
        parts
            .into_iter()
            .map(|part| match part {
                SurfaceTextPart::Static(text) => RenderTextSegment::Static(text),
                SurfaceTextPart::Dynamic(expr) => RenderTextSegment::Expr(self.intern_template_expr(&expr)),
            })
            .collect()
    }

    fn intern_template_expr(&mut self, source: &str) -> RenderExprId {
        let trimmed = source.trim();
        if let Some(id) = self.expr_intern.get(trimmed).copied() {
            return id;
        }
        let id = RenderExprId(self.module.exprs.len() as u32);
        self.module.exprs.push(build_expr_record(trimmed, RenderExprKind::Template, &self.script_bindings, &self.binding_name_to_id));
        self.expr_intern.insert(trimmed.to_string(), id);
        id
    }

    fn alloc_region(&mut self, nodes: Vec<RenderNodeId>) -> RenderRegionId {
        if nodes.is_empty() {
            return RenderRegionId::EMPTY;
        }
        let id = RenderRegionId(self.module.regions.len() as u32);
        self.module.regions.push(RenderRegion { nodes });
        id
    }
}

fn build_expr_record(
    source: &str,
    kind: RenderExprKind,
    script_bindings: &[ScriptBinding],
    binding_name_to_id: &HashMap<String, RenderBindingId>,
) -> RenderExpr {
    let binding_refs = resolve_binding_refs(source, script_bindings, binding_name_to_id);
    RenderExpr {
        kind,
        source: source.to_string(),
        binding_refs,
        value_kind: infer_value_kind(source),
        purity: infer_purity(source, kind),
        memoizable: matches!(kind, RenderExprKind::BindingInit) || is_likely_pure(source),
        event_call: None,
    }
}

fn resolve_binding_refs(
    source: &str,
    script_bindings: &[ScriptBinding],
    binding_name_to_id: &HashMap<String, RenderBindingId>,
) -> Vec<RenderBindingId> {
    let reactive_names: std::collections::BTreeSet<_> = script_bindings.iter().filter(|b| b.reactive).map(|b| b.name.as_str()).collect();
    super::super::expr_deps::extract_expr_idents(source)
        .into_iter()
        .filter(|name| reactive_names.contains(name.as_str()))
        .filter_map(|name| binding_name_to_id.get(&name).copied())
        .collect()
}

fn infer_value_kind(source: &str) -> RenderValueKind {
    let trimmed = source.trim();
    if trimmed == "true" || trimmed == "false" {
        return RenderValueKind::Bool;
    }
    if trimmed.parse::<i32>().is_ok() {
        return RenderValueKind::I32;
    }
    if (trimmed.starts_with('"') && trimmed.ends_with('"')) || (trimmed.starts_with('\'') && trimmed.ends_with('\'')) {
        return RenderValueKind::Utf8;
    }
    if trimmed.starts_with('[') {
        return RenderValueKind::List;
    }
    RenderValueKind::Unknown
}

fn infer_purity(source: &str, kind: RenderExprKind) -> RenderPurity {
    if matches!(kind, RenderExprKind::EventHandler) {
        return RenderPurity::Impure;
    }
    if source.contains('(') || source.contains("=>") {
        return RenderPurity::Unknown;
    }
    if is_likely_pure(source) { RenderPurity::Pure } else { RenderPurity::Unknown }
}

fn is_likely_pure(source: &str) -> bool {
    let trimmed = source.trim();
    trimmed.parse::<i32>().is_ok() || trimmed == "true" || trimmed == "false" || trimmed.starts_with('"')
}

/// Lower surface RenderIR and script bindings to canonical RenderIR.
pub fn canonicalize_surface(surface: SurfaceIr, script_bindings: &[ScriptBinding]) -> RenderModule {
    Canonicalizer::new(script_bindings).finish(surface)
}
