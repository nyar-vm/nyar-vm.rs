//! RenderIR → WXML 字符串与绑定表。

use std::fmt::Write as _;

use crate::awsl::{
    LoweredComponent, RenderAttr, RenderAttrValue, RenderIfNode, RenderLoopNode, RenderModule, RenderNode, RenderNodeId, RenderRegionId,
    RenderTextSegment, ScriptBinding, TemplateNodeKind,
    render_ir::{attr_value_source, region_nodes},
};

/// WXML 页面输出。
#[derive(Debug, Clone)]
pub struct MpWxmlOutput {
    /// 页面路由名。
    pub route_name: String,
    /// WXML 内容。
    pub content: String,
    /// 相对路径（如 `pages/index/index.wxml`）。
    pub relative_path: String,
    /// 需要注入到 `data` 的响应式字段名。
    pub reactive_fields: Vec<String>,
}

/// 将组件 RenderIR 生成为 WXML。
pub fn generate_page_wxml(component: &LoweredComponent) -> MpWxmlOutput {
    let mut out = String::new();
    let mut ctx = MpRenderCtx::new(&component.render_ir, &component.script_bindings);
    ctx.emit_roots(&mut out, 0);
    let reactive_fields = component.script_bindings.iter().filter(|b| b.reactive).map(|b| b.name.clone()).collect();
    let route = &component.route_name;
    MpWxmlOutput { route_name: route.clone(), content: out, relative_path: format!("pages/{route}/{route}.wxml"), reactive_fields }
}

struct MpRenderCtx<'a> {
    module: &'a RenderModule,
    bindings: &'a [ScriptBinding],
    indent: usize,
}

impl<'a> MpRenderCtx<'a> {
    fn new(module: &'a RenderModule, bindings: &'a [ScriptBinding]) -> Self {
        Self { module, bindings, indent: 0 }
    }

    fn emit_roots(&mut self, out: &mut String, depth: usize) {
        self.indent = depth;
        self.emit_region(out, &self.module.roots);
    }

    fn emit_region(&mut self, out: &mut String, node_ids: &[RenderNodeId]) {
        for &node_id in node_ids {
            self.emit_node(out, node_id);
        }
    }

    fn emit_region_id(&mut self, out: &mut String, region: RenderRegionId) {
        self.emit_region(out, region_nodes(self.module, region));
    }

    fn emit_node(&mut self, out: &mut String, node_id: RenderNodeId) {
        match self.module.node(node_id) {
            RenderNode::Element(element) => {
                let wx_tag = map_tag(&element.tag, element.kind);
                let children = region_nodes(self.module, element.children);
                self.write_indent(out);
                write!(out, "<{wx_tag}").unwrap();
                self.emit_attrs(out, &element.attrs);
                if children.is_empty() {
                    writeln!(out, " />").unwrap();
                }
                else {
                    writeln!(out, ">").unwrap();
                    self.indent += 1;
                    self.emit_region(out, children);
                    self.indent -= 1;
                    self.write_indent(out);
                    writeln!(out, "</{wx_tag}>").unwrap();
                }
            }
            RenderNode::Component(component) => {
                let wx_tag = map_tag(&component.tag, TemplateNodeKind::Component);
                let children = region_nodes(self.module, component.children);
                self.write_indent(out);
                write!(out, "<{wx_tag}").unwrap();
                self.emit_attrs(out, &component.attrs);
                if children.is_empty() {
                    writeln!(out, " />").unwrap();
                }
                else {
                    writeln!(out, ">").unwrap();
                    self.indent += 1;
                    self.emit_region(out, children);
                    self.indent -= 1;
                    self.write_indent(out);
                    writeln!(out, "</{wx_tag}>").unwrap();
                }
            }
            RenderNode::Text(text) => {
                let body = self.emit_text_segments(&text.segments);
                if !body.is_empty() {
                    self.write_indent(out);
                    writeln!(out, "<text>{body}</text>").unwrap();
                }
            }
            RenderNode::If(render_if) => self.emit_if(out, render_if),
            RenderNode::Loop(render_loop) => self.emit_loop(out, render_loop),
            RenderNode::Fragment(fragment) => self.emit_region_id(out, fragment.children),
        }
    }

    fn emit_if(&mut self, out: &mut String, render_if: &RenderIfNode) {
        self.write_indent(out);
        let cond = self.rewrite_expr(self.module.expr_source(render_if.condition));
        writeln!(out, "<block wx:if=\"{{{{{cond}}}}}\">").unwrap();
        self.indent += 1;
        self.emit_region_id(out, render_if.then_region);
        self.indent -= 1;
        if !region_nodes(self.module, render_if.else_region).is_empty() {
            self.write_indent(out);
            writeln!(out, "<block wx:else>").unwrap();
            self.indent += 1;
            self.emit_region_id(out, render_if.else_region);
            self.indent -= 1;
            self.write_indent(out);
            writeln!(out, "</block>").unwrap();
        }
        self.write_indent(out);
        writeln!(out, "</block>").unwrap();
    }

    fn emit_loop(&mut self, out: &mut String, render_loop: &RenderLoopNode) {
        self.write_indent(out);
        let items = self.rewrite_expr(self.module.expr_source(render_loop.items));
        let item_var = &render_loop.item_var;
        let key = render_loop.key.map(|key_id| self.rewrite_expr(self.module.expr_source(key_id))).unwrap_or_else(|| "index".to_string());
        writeln!(out, "<block wx:for=\"{{{{{items}}}}}\" wx:for-item=\"{item_var}\" wx:key=\"{key}\">").unwrap();
        self.indent += 1;
        self.emit_region_id(out, render_loop.body_region);
        self.indent -= 1;
        self.write_indent(out);
        writeln!(out, "</block>").unwrap();
    }

    fn emit_attrs(&self, out: &mut String, attrs: &[RenderAttr]) {
        for attr in attrs {
            if attr.is_event {
                let handler = attr_value_source(self.module, &attr.value);
                write!(out, " bind:tap=\"{handler}\"").unwrap();
                continue;
            }
            if attr.name == "class" || attr.name == ":class" {
                match &attr.value {
                    RenderAttrValue::Static(value) => {
                        write!(out, " class=\"{}\"", escape_attr(value)).unwrap();
                    }
                    RenderAttrValue::Expr(expr_id) => {
                        let rewritten = self.rewrite_expr(self.module.expr_source(*expr_id));
                        write!(out, " class=\"{{{{{rewritten}}}}}\"").unwrap();
                    }
                    RenderAttrValue::Template(segments) => {
                        let text = self.emit_text_segments(segments);
                        write!(out, " class=\"{text}\"").unwrap();
                    }
                }
                continue;
            }
            if attr.is_prop {
                match &attr.value {
                    RenderAttrValue::Static(value) => {
                        write!(out, " {}=\"{}\"", attr.name, escape_attr(value)).unwrap();
                    }
                    RenderAttrValue::Expr(expr_id) => {
                        let rewritten = self.rewrite_expr(self.module.expr_source(*expr_id));
                        write!(out, " {}=\"{{{{{rewritten}}}}}\"", attr.name).unwrap();
                    }
                    RenderAttrValue::Template(segments) => {
                        let text = self.emit_text_segments(segments);
                        write!(out, " {}=\"{text}\"", attr.name).unwrap();
                    }
                }
                continue;
            }
            match &attr.value {
                RenderAttrValue::Static(value) => write!(out, " {}=\"{}\"", attr.name, escape_attr(value)).unwrap(),
                RenderAttrValue::Expr(expr_id) => {
                    let rewritten = self.rewrite_expr(self.module.expr_source(*expr_id));
                    write!(out, " {}=\"{{{{{rewritten}}}}}\"", attr.name).unwrap();
                }
                RenderAttrValue::Template(segments) => {
                    let text = self.emit_text_segments(segments);
                    write!(out, " {}=\"{text}\"", attr.name).unwrap();
                }
            }
        }
    }

    fn emit_text_segments(&self, segments: &[RenderTextSegment]) -> String {
        let mut out = String::new();
        for segment in segments {
            match segment {
                RenderTextSegment::Static(text) => out.push_str(text),
                RenderTextSegment::Expr(expr_id) => {
                    let rewritten = self.rewrite_expr(self.module.expr_source(*expr_id));
                    write!(out, "{{{{{rewritten}}}}}").unwrap();
                }
            }
        }
        out
    }

    fn rewrite_expr(&self, expr: &str) -> String {
        let mut out = expr.trim().to_string();
        for binding in self.bindings.iter().filter(|b| b.reactive) {
            out = out.replace(&binding.name, &format!("{}", binding.name));
        }
        out
    }

    fn write_indent(&self, out: &mut String) {
        for _ in 0..self.indent {
            out.push_str("  ");
        }
    }
}

fn map_tag(tag: &str, kind: TemplateNodeKind) -> &'static str {
    match kind {
        TemplateNodeKind::Component => "view",
        TemplateNodeKind::Intrinsic => match tag {
            "Text" | "text" => "text",
            "Button" | "button" => "button",
            "Slot" | "slot" => "slot",
            _ => "view",
        },
        TemplateNodeKind::HostView => match tag {
            "button" => "button",
            "text" | "span" => "text",
            "img" | "image" => "image",
            "input" => "input",
            _ => "view",
        },
    }
}

fn escape_attr(value: &str) -> String {
    value.replace('&', "&amp;").replace('"', "&quot;")
}
#[cfg(test)]
#[path = "../../tests/path_wired/codegen_mp_render.rs"]
mod codegen_mp_render_tests;
