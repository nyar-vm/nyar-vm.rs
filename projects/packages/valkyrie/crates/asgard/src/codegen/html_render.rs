//! RenderIR → 静态 HTML（SSG / 文档站，无 hydration）。

use std::fmt::Write as _;

use oak_awsl::is_html_void_element;
use serde_json::Value;

use crate::{
    awsl::{
        LoweredComponent, RenderAttr, RenderAttrValue, RenderLoopNode, RenderModule, RenderNode, RenderNodeId, RenderRegionId,
        RenderTextSegment, ScriptBinding, TemplateNodeKind,
        render_ir::{attr_value_source, region_nodes},
    },
    codegen::expr_eval::ExprContext,
};

/// 静态 HTML 渲染结果。
#[derive(Debug, Clone)]
pub struct StaticRenderResult {
    /// 组件 HTML 片段（含根 `div` 包装）。
    pub html: String,
    /// 作用域 CSS。
    pub css: String,
    /// 作用域 class 前缀。
    pub scope: String,
}

/// 将已降级的组件 RenderIR 渲染为静态 HTML。
pub fn render_static_ir(component: &LoweredComponent, data: &Value) -> StaticRenderResult {
    let scope = format!("asgard-static-{}", to_kebab_case(&component.name));
    let mut html_body = String::new();
    let mut ctx = HtmlRenderCtx::new(component, data, &scope);
    ctx.emit_roots(&mut html_body, &component.render_ir);

    let css = component.style.as_deref().unwrap_or("").to_string();
    let html = format!(
        "<div class=\"{scope}\" data-asgard-static=\"{name}\">\n{html_body}\n</div>",
        scope = scope,
        name = escape_attr(&component.route_name),
        html_body = html_body.trim_end()
    );

    StaticRenderResult { html, css, scope }
}

struct HtmlRenderCtx<'a> {
    module: &'a RenderModule,
    expr: ExprContext,
    scope: &'a str,
}

impl<'a> HtmlRenderCtx<'a> {
    fn new(component: &'a LoweredComponent, data: &Value, scope: &'a str) -> Self {
        Self { module: &component.render_ir, expr: ExprContext::from_bindings(&component.script_bindings, data), scope }
    }

    fn emit_roots(&mut self, out: &mut String, module: &RenderModule) {
        self.emit_region(out, module, &module.roots);
    }

    fn emit_region(&mut self, out: &mut String, module: &RenderModule, node_ids: &[RenderNodeId]) {
        for &node_id in node_ids {
            self.emit_node(out, module, node_id);
        }
    }

    fn emit_region_id(&mut self, out: &mut String, module: &RenderModule, region: RenderRegionId) {
        self.emit_region(out, module, region_nodes(module, region));
    }

    fn emit_node(&mut self, out: &mut String, module: &RenderModule, node_id: RenderNodeId) {
        match module.node(node_id) {
            RenderNode::Element(element) => {
                let tag = &element.tag;
                let kind = element.kind;
                if matches!(kind, TemplateNodeKind::Intrinsic) && tag.eq_ignore_ascii_case("slot") {
                    return;
                }
                if tag.eq_ignore_ascii_case("head") || tag.eq_ignore_ascii_case("script") {
                    return;
                }
                let html_tag = map_html_tag(tag, kind);
                write!(out, "<{html_tag}").unwrap();
                self.emit_attrs(out, module, &element.attrs);
                let children = region_nodes(module, element.children);
                if children.is_empty() && is_html_void_element(&html_tag) {
                    write!(out, " />").unwrap();
                }
                else {
                    write!(out, ">").unwrap();
                    self.emit_region(out, module, children);
                    write!(out, "</{html_tag}>").unwrap();
                }
            }
            RenderNode::Component(component) => {
                let html_tag = map_html_tag(&component.tag, TemplateNodeKind::Component);
                write!(out, "<{html_tag}").unwrap();
                self.emit_attrs(out, module, &component.attrs);
                write!(out, ">").unwrap();
                self.emit_region_id(out, module, component.children);
                write!(out, "</{html_tag}>").unwrap();
            }
            RenderNode::Text(text) => {
                let body = self.emit_text_segments(module, &text.segments);
                out.push_str(&escape_html(&body));
            }
            RenderNode::If(render_if) => {
                if self.expr.eval_truthy(module.expr_source(render_if.condition)) {
                    self.emit_region_id(out, module, render_if.then_region);
                }
                else {
                    self.emit_region_id(out, module, render_if.else_region);
                }
            }
            RenderNode::Loop(render_loop) => self.emit_loop(out, module, render_loop),
            RenderNode::Fragment(fragment) => self.emit_region_id(out, module, fragment.children),
        }
    }

    fn emit_loop(&mut self, out: &mut String, module: &RenderModule, render_loop: &RenderLoopNode) {
        let items = self.expr.eval_value(module.expr_source(render_loop.items));
        let Some(array) = items.as_array()
        else {
            return;
        };
        for (index, item) in array.iter().enumerate() {
            self.expr.push_loop(&render_loop.item_var, &render_loop.index_var, item.clone(), index);
            self.emit_region_id(out, module, render_loop.body_region);
            self.expr.pop_loop();
        }
    }

    fn emit_attrs(&self, out: &mut String, module: &RenderModule, attrs: &[RenderAttr]) {
        for attr in attrs {
            if attr.is_event {
                continue;
            }
            let name = normalize_attr_name(&attr.name);
            if name.is_empty() {
                continue;
            }
            let value = match &attr.value {
                RenderAttrValue::Static(text) => text.clone(),
                RenderAttrValue::Expr(expr_id) => self.expr.eval_string(module.expr_source(*expr_id)),
                RenderAttrValue::Template(segments) => self.emit_text_segments(module, segments),
            };
            if !value.is_empty() || name == "class" || name == "href" {
                write!(out, " {name}=\"{}\"", escape_attr(&value)).unwrap();
            }
        }
    }

    fn emit_text_segments(&self, module: &RenderModule, segments: &[RenderTextSegment]) -> String {
        let mut out = String::new();
        for segment in segments {
            match segment {
                RenderTextSegment::Static(text) => out.push_str(text),
                RenderTextSegment::Expr(expr_id) => out.push_str(&self.expr.eval_string(module.expr_source(*expr_id))),
            }
        }
        out
    }
}

fn map_html_tag(tag: &str, kind: TemplateNodeKind) -> String {
    match kind {
        TemplateNodeKind::Component if tag.eq_ignore_ascii_case("link") => "a".into(),
        TemplateNodeKind::Component => "div".into(),
        TemplateNodeKind::Intrinsic => match tag {
            "Text" | "text" => "span".into(),
            "Button" | "button" => "button".into(),
            "Column" | "column" | "Box" | "box" | "Row" | "row" | "Flex" | "flex" => "div".into(),
            "Slot" | "slot" => "div".into(),
            other => other.to_ascii_lowercase(),
        },
        TemplateNodeKind::HostView => tag.to_ascii_lowercase(),
    }
}

fn normalize_attr_name(name: &str) -> String {
    match name {
        "href" | "class" | "id" | "title" | "rel" | "target" | "type" | "name" | "content" => name.to_string(),
        n if n.starts_with(':') => n[1..].to_string(),
        n if n.starts_with('@') => String::new(),
        n if n == "on:click" || n.starts_with("on:") => String::new(),
        n => n.to_string(),
    }
}

fn to_kebab_case(name: &str) -> String {
    let mut out = String::new();
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
        }
        else if ch == '_' {
            out.push('-');
        }
        else {
            out.push(ch);
        }
    }
    out
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn escape_attr(text: &str) -> String {
    escape_html(text).replace('"', "&quot;")
}
#[cfg(test)]
#[path = "../../tests/path_wired/codegen_html_render.rs"]
mod codegen_html_render_tests;
