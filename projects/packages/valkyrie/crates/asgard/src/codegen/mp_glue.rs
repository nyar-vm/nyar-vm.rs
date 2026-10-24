//! 小程序页面 WASM 胶水（`require` + 事件转发到 `awsl_call_*`）。

use std::fmt::Write as _;

use crate::{
    awsl::{
        LoweredComponent, RenderIr, RenderModule, RenderNode, RenderNodeId,
        render_ir::{attr_value_source, region_nodes},
    },
    codegen::ui_host_abi::resolve_call_export,
};

/// 小程序胶水输出（事件处理器片段，合并进 page js）。
#[derive(Debug, Clone)]
pub struct MpGlueOutput {
    /// 组件名。
    pub component_name: String,
    /// 页面 JS 事件转发片段。
    pub event_forwarders: String,
    /// 相对路径。
    pub relative_path: String,
}

/// 为页面生成事件处理器转发片段。
pub fn generate_page_glue(component: &LoweredComponent) -> MpGlueOutput {
    let route = &component.route_name;
    let handlers = collect_event_handlers(&component.render_ir);
    let mut out = String::new();
    writeln!(out, "// event forwarders for {route}").unwrap();
    for name in handlers {
        let export = resolve_call_export(&name);
        writeln!(out, "  {name}(e) {{").unwrap();
        writeln!(out, "    asgard.on_event('{name}');").unwrap();
        writeln!(out, "  }},").unwrap();
        let _ = export;
    }
    MpGlueOutput { component_name: component.name.clone(), event_forwarders: out, relative_path: format!("pages/{route}/{route}.glue.js") }
}

fn collect_event_handlers(module: &RenderIr) -> Vec<String> {
    let mut handlers = Vec::new();
    for &root_id in &module.roots {
        collect_event_handlers_node(module, root_id, &mut handlers);
    }
    handlers.sort();
    handlers.dedup();
    handlers
}

fn collect_event_handlers_region(module: &RenderModule, region: crate::awsl::RenderRegionId, handlers: &mut Vec<String>) {
    for &node_id in region_nodes(module, region) {
        collect_event_handlers_node(module, node_id, handlers);
    }
}

fn collect_event_handlers_node(module: &RenderModule, node_id: RenderNodeId, handlers: &mut Vec<String>) {
    match module.node(node_id) {
        RenderNode::Element(element) => {
            for attr in &element.attrs {
                if attr.is_event {
                    handlers.push(attr_value_source(module, &attr.value));
                }
            }
            collect_event_handlers_region(module, element.children, handlers);
        }
        RenderNode::Component(component) => {
            for attr in &component.attrs {
                if attr.is_event {
                    handlers.push(attr_value_source(module, &attr.value));
                }
            }
            collect_event_handlers_region(module, component.children, handlers);
        }
        RenderNode::Text { .. } => {}
        RenderNode::If(render_if) => {
            collect_event_handlers_region(module, render_if.then_region, handlers);
            collect_event_handlers_region(module, render_if.else_region, handlers);
        }
        RenderNode::Loop(render_loop) => collect_event_handlers_region(module, render_loop.body_region, handlers),
        RenderNode::Fragment(fragment) => collect_event_handlers_region(module, fragment.children, handlers),
    }
}
