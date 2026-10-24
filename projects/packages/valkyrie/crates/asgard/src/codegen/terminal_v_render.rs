//! RenderIR → 终端 V widget 构建函数（纯 Valkyrie widget_* 调用）。
//!
//! 终端路径不使用 DOM / 信号系统：widget 树由 `widget_*` 原语构建，状态由
//! `TuiRuntime` 的绑定存储管理。本模块从 RenderIR 生成：
//!
//! - `awsl_init_<route>()`：注册所有 `let mut` 绑定到 TuiRuntime
//! - `awsl_mount_<route>()`：构建 widget 树（`ArrayList<Widget>`）
//! - `awsl_call_*()`：事件处理器包装（调用原始 handler + 重挂载）
//! - `awsl_main()`：初始化绑定 → 挂载 → 启动事件循环

use std::fmt::Write as _;

use crate::awsl::{
    LoweredComponent, RenderAttr, RenderAttrValue, RenderIfNode, RenderLoopNode, RenderModule, RenderNode, RenderNodeId, RenderRegionId,
    RenderTextSegment, ScriptBinding,
    render_ir::{attr_value_source, region_nodes},
};

/// 将组件 RenderIR 生成为终端 V 源码（init + mount + handlers + main）。
pub fn render_terminal_component_v(component: &LoweredComponent) -> String {
    let route = sanitize(&component.route_name);
    let mut out = String::new();
    let mut ctx = TerminalVCodegen {
        route: &route,
        module: &component.render_ir,
        bindings: &component.script_bindings,
        var_counter: 0,
        handler_names: Vec::new(),
    };
    out.push_str(&emit_init(&route, &component.script_bindings));
    out.push_str(&emit_mount(&mut ctx));
    out.push_str(&emit_handlers(&route, &ctx.handler_names));
    out.push_str(&emit_main(&route));
    out
}

struct TerminalVCodegen<'a> {
    route: &'a str,
    module: &'a RenderModule,
    bindings: &'a [ScriptBinding],
    var_counter: u32,
    handler_names: Vec<String>,
}

impl<'a> TerminalVCodegen<'a> {
    fn next_var(&mut self) -> String {
        let name = format!("w{}", self.var_counter);
        self.var_counter += 1;
        name
    }

    fn collect_handler(&mut self, name: &str) {
        let full = format!("awsl_call_{name}");
        if !self.handler_names.contains(&full) {
            self.handler_names.push(full);
        }
    }
}

/// 生成 `awsl_init_<route>()`：注册所有响应式绑定的初始值。
fn emit_init(route: &str, bindings: &[ScriptBinding]) -> String {
    let mut out = String::new();
    writeln!(out, "# 绑定初始化：注册所有 let mut 变量到 TuiRuntime").unwrap();
    writeln!(out, "micro awsl_init_{route}() {{").unwrap();
    for b in bindings.iter().filter(|b| b.reactive) {
        let init = sanitize_v_string(&b.init_expr);
        writeln!(out, "    tui_register_binding(\"{}\", \"{init}\")", b.name).unwrap();
    }
    writeln!(out, "}}\n").unwrap();
    out
}

/// 生成 `awsl_mount_<route>()`：构建 widget 树。
fn emit_mount(ctx: &mut TerminalVCodegen<'_>) -> String {
    let mut out = String::new();
    writeln!(out, "# widget 树构建：从 RenderIR 生成 widget_* 调用").unwrap();
    writeln!(out, "micro awsl_mount_{}() -> ArrayList<Widget> {{", ctx.route).unwrap();
    writeln!(out, "    let roots: ArrayList<Widget> = ArrayList::new(16)").unwrap();
    for &root_id in &ctx.module.roots {
        let var = emit_node(ctx, &mut out, root_id, "    ");
        if !var.is_empty() {
            writeln!(out, "    roots.push({var})").unwrap();
        }
    }
    writeln!(out, "    return roots").unwrap();
    writeln!(out, "}}\n").unwrap();
    out
}

/// 生成单个节点，返回变量名（空字符串表示不需要 push）。
fn emit_node(ctx: &mut TerminalVCodegen<'_>, out: &mut String, node_id: RenderNodeId, indent: &str) -> String {
    match ctx.module.node(node_id) {
        RenderNode::Element(element) => emit_tag(ctx, out, &element.tag, &element.attrs, element.children, indent),
        RenderNode::Component(component) => emit_tag(ctx, out, &component.tag, &component.attrs, component.children, indent),
        RenderNode::Text(text) => emit_text(ctx, out, &text.segments, indent),
        RenderNode::Fragment(fragment) => emit_fragment(ctx, out, fragment.children, indent),
        RenderNode::If(render_if) => emit_if(ctx, out, render_if, indent),
        RenderNode::Loop(render_loop) => emit_loop(ctx, out, render_loop, indent),
    }
}

fn emit_tag(
    ctx: &mut TerminalVCodegen<'_>,
    out: &mut String,
    tag: &str,
    attrs: &[RenderAttr],
    children: RenderRegionId,
    indent: &str,
) -> String {
    let lower = tag.to_ascii_lowercase();
    match lower.as_str() {
        "column" => emit_container(ctx, out, "widget_column", children, indent),
        "row" => emit_container(ctx, out, "widget_row", children, indent),
        "box" => emit_container(ctx, out, "widget_box", children, indent),
        "list" => emit_container(ctx, out, "widget_list", children, indent),
        "text" => {
            let text_expr = extract_text_expr(ctx.module, children);
            let var = ctx.next_var();
            writeln!(out, "{indent}let {var}: Widget = widget_text({text_expr})").unwrap();
            var
        }
        "button" => {
            let label_expr = extract_text_expr(ctx.module, children);
            let event = extract_event(ctx.module, attrs).unwrap_or_default();
            if !event.is_empty() {
                ctx.collect_handler(&event);
            }
            let var = ctx.next_var();
            writeln!(out, "{indent}let {var}: Widget = widget_button({label_expr}, \"{event}\")").unwrap();
            var
        }
        "item" => {
            let label_expr = extract_text_expr(ctx.module, children);
            let event = extract_event(ctx.module, attrs).unwrap_or_default();
            if !event.is_empty() {
                ctx.collect_handler(&event);
            }
            let var = ctx.next_var();
            writeln!(out, "{indent}let {var}: Widget = widget_item({label_expr}, \"{event}\")").unwrap();
            var
        }
        "checkbox" => {
            let label_expr = extract_text_expr(ctx.module, children);
            let event = extract_event(ctx.module, attrs).unwrap_or_default();
            let binding = extract_binding_name(ctx.module, attrs).unwrap_or_default();
            if !event.is_empty() {
                ctx.collect_handler(&event);
            }
            let var = ctx.next_var();
            writeln!(out, "{indent}let {var}: Widget = widget_checkbox({label_expr}, \"{event}\", \"{binding}\")").unwrap();
            var
        }
        "radio" => {
            let label_expr = extract_text_expr(ctx.module, children);
            let event = extract_event(ctx.module, attrs).unwrap_or_default();
            let binding = extract_binding_name(ctx.module, attrs).unwrap_or_default();
            let option_val = extract_option_value(ctx.module, attrs).unwrap_or_default();
            if !event.is_empty() {
                ctx.collect_handler(&event);
            }
            let var = ctx.next_var();
            writeln!(out, "{indent}let {var}: Widget = widget_radio({label_expr}, \"{event}\", \"{binding}\", \"{option_val}\")").unwrap();
            var
        }
        "radiogroup" => emit_container(ctx, out, "widget_column", children, indent),
        "flex" | "slot" => emit_container(ctx, out, "widget_column", children, indent),
        _ => {
            let text_expr = extract_text_expr(ctx.module, children);
            let var = ctx.next_var();
            writeln!(out, "{indent}let {var}: Widget = widget_text({text_expr})").unwrap();
            var
        }
    }
}

/// 生成容器 widget（column/row/box/list）：先构建子节点列表，再包装。
fn emit_container(ctx: &mut TerminalVCodegen<'_>, out: &mut String, ctor: &str, children: RenderRegionId, indent: &str) -> String {
    let list_var = format!("children_{}", ctx.var_counter);
    ctx.var_counter += 1;
    writeln!(out, "{indent}let {list_var}: ArrayList<Widget> = ArrayList::new(8)").unwrap();
    for &child_id in region_nodes(ctx.module, children) {
        let child_var = emit_node(ctx, out, child_id, indent);
        if !child_var.is_empty() {
            writeln!(out, "{indent}{list_var}.push({child_var})").unwrap();
        }
    }
    let var = ctx.next_var();
    writeln!(out, "{indent}let {var}: Widget = {ctor}({list_var})").unwrap();
    var
}

fn emit_fragment(ctx: &mut TerminalVCodegen<'_>, out: &mut String, children: RenderRegionId, indent: &str) -> String {
    let child_ids = region_nodes(ctx.module, children);
    if child_ids.len() == 1 {
        return emit_node(ctx, out, child_ids[0], indent);
    }
    emit_container(ctx, out, "widget_column", children, indent)
}

fn emit_if(ctx: &mut TerminalVCodegen<'_>, out: &mut String, render_if: &RenderIfNode, indent: &str) -> String {
    let var = ctx.next_var();
    let cond = rewrite_cond_expr(ctx.module.expr_source(render_if.condition), ctx.bindings);
    writeln!(out, "{indent}let {var}: Widget = widget_text(\"\")").unwrap();
    writeln!(out, "{indent}if {cond} {{").unwrap();
    {
        let inner = format!("{indent}    ");
        for &node_id in region_nodes(ctx.module, render_if.then_region) {
            let child_var = emit_node(ctx, out, node_id, &inner);
            if !child_var.is_empty() {
                writeln!(out, "{inner}{var} = {child_var}").unwrap();
                break;
            }
        }
    }
    writeln!(out, "{indent}}} else {{").unwrap();
    {
        let inner = format!("{indent}    ");
        for &node_id in region_nodes(ctx.module, render_if.else_region) {
            let child_var = emit_node(ctx, out, node_id, &inner);
            if !child_var.is_empty() {
                writeln!(out, "{inner}{var} = {child_var}").unwrap();
                break;
            }
        }
    }
    writeln!(out, "{indent}}}").unwrap();
    var
}

fn emit_loop(ctx: &mut TerminalVCodegen<'_>, out: &mut String, render_loop: &RenderLoopNode, indent: &str) -> String {
    let list_var = format!("loop_{}", ctx.var_counter);
    ctx.var_counter += 1;
    writeln!(out, "{indent}let {list_var}: ArrayList<Widget> = ArrayList::new(8)").unwrap();
    writeln!(out, "{indent}# loop: {} in {}", render_loop.item_var, ctx.module.expr_source(render_loop.items)).unwrap();
    let var = ctx.next_var();
    writeln!(out, "{indent}let {var}: Widget = widget_list({list_var})").unwrap();
    var
}

fn emit_text(ctx: &mut TerminalVCodegen<'_>, out: &mut String, segments: &[RenderTextSegment], indent: &str) -> String {
    let expr = build_text_expr(ctx.module, segments, ctx.bindings);
    let var = ctx.next_var();
    writeln!(out, "{indent}let {var}: Widget = widget_text({expr})").unwrap();
    var
}

fn extract_text_expr(module: &RenderModule, region: RenderRegionId) -> String {
    let mut segments: Vec<RenderTextSegment> = Vec::new();
    for &node_id in region_nodes(module, region) {
        match module.node(node_id) {
            RenderNode::Text(text) => segments.extend(text.segments.clone()),
            RenderNode::Element(element) => {
                let sub = extract_text_expr(module, element.children);
                if !sub.is_empty() {
                    segments.push(RenderTextSegment::Static(sub));
                }
            }
            RenderNode::Component(component) => {
                let sub = extract_text_expr(module, component.children);
                if !sub.is_empty() {
                    segments.push(RenderTextSegment::Static(sub));
                }
            }
            RenderNode::Fragment(fragment) => {
                let sub = extract_text_expr(module, fragment.children);
                if !sub.is_empty() {
                    segments.push(RenderTextSegment::Static(sub));
                }
            }
            _ => {}
        }
    }
    if segments.is_empty() {
        return "\"\"".to_string();
    }
    build_text_expr(module, &segments, &[])
}

fn build_text_expr(module: &RenderModule, segments: &[RenderTextSegment], bindings: &[ScriptBinding]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for segment in segments {
        match segment {
            RenderTextSegment::Static(text) => {
                if !text.is_empty() {
                    parts.push(format!("\"{}\"", sanitize_v_string(text)));
                }
            }
            RenderTextSegment::Expr(expr_id) => {
                let name = module.expr_source(*expr_id).trim();
                parts.push(format!("tui_binding_get(\"{name}\")"));
            }
        }
    }
    if parts.is_empty() {
        return "\"\"".to_string();
    }
    parts.join(" + ")
}

fn extract_event(module: &RenderModule, attrs: &[RenderAttr]) -> Option<String> {
    for attr in attrs {
        if attr.is_event && (attr.name == "@click" || attr.name == "on:click") {
            return Some(attr_value_source(module, &attr.value));
        }
    }
    None
}

fn extract_binding_name(module: &RenderModule, attrs: &[RenderAttr]) -> Option<String> {
    for attr in attrs {
        if attr.name == ":binding" || attr.name == "binding" {
            return attr_value_string(module, &attr.value);
        }
    }
    None
}

fn extract_option_value(module: &RenderModule, attrs: &[RenderAttr]) -> Option<String> {
    for attr in attrs {
        if attr.name == ":option" || attr.name == "option" || attr.name == "value" {
            return attr_value_string(module, &attr.value);
        }
    }
    None
}

fn attr_value_string(module: &RenderModule, value: &RenderAttrValue) -> Option<String> {
    match value {
        RenderAttrValue::Static(text) => Some(text.clone()),
        RenderAttrValue::Expr(expr_id) => Some(module.expr_source(*expr_id).to_string()),
        RenderAttrValue::Template(segments) => Some(
            segments
                .iter()
                .map(|segment| match segment {
                    RenderTextSegment::Static(text) => text.clone(),
                    RenderTextSegment::Expr(expr_id) => module.expr_source(*expr_id).to_string(),
                })
                .collect(),
        ),
    }
}

fn rewrite_cond_expr(expr: &str, bindings: &[ScriptBinding]) -> String {
    let trimmed = expr.trim();
    for b in bindings {
        if trimmed == b.name {
            return format!("tui_binding_truthy(\"{trimmed}\")");
        }
    }
    format!("tui_binding_truthy(\"{trimmed}\")")
}

/// 生成事件处理器：原始 handler 桩 + awsl_call_* 包装（调用后重挂载）。
fn emit_handlers(route: &str, handler_names: &[String]) -> String {
    let mut out = String::new();
    if handler_names.is_empty() {
        return out;
    }
    writeln!(out, "# 原始事件处理器桩：用户填写绑定操作逻辑").unwrap();
    for full_name in handler_names {
        let original = full_name.strip_prefix("awsl_call_").unwrap_or(full_name);
        writeln!(out, "micro {original}() {{").unwrap();
        writeln!(out, "    # TODO: 在此用 tui_binding_set / tui_register_binding 更新状态").unwrap();
        writeln!(out, "}}\n").unwrap();
    }
    writeln!(out, "# 事件处理器包装：调用原始 handler 后重挂载 widget 树").unwrap();
    for full_name in handler_names {
        let original = full_name.strip_prefix("awsl_call_").unwrap_or(full_name);
        writeln!(out, "micro {full_name}() {{").unwrap();
        writeln!(out, "    {original}()").unwrap();
        writeln!(out, "    tui_remount(awsl_mount_{route}())").unwrap();
        writeln!(out, "}}\n").unwrap();
    }
    out
}

fn emit_main(route: &str) -> String {
    let mut out = String::new();
    writeln!(out, "# 主入口：初始化 → 挂载 → 启动事件循环").unwrap();
    writeln!(out, "micro awsl_main() -> i32 {{").unwrap();
    writeln!(out, "    awsl_init_{route}()").unwrap();
    writeln!(out, "    let roots: ArrayList<Widget> = awsl_mount_{route}()").unwrap();
    writeln!(out, "    return asgard_terminal_run(roots)").unwrap();
    writeln!(out, "}}").unwrap();
    out
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '_' { ch } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_ascii_lowercase()
}

fn sanitize_v_string(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}
#[cfg(test)]
#[path = "../../tests/path_wired/codegen_terminal_v_render.rs"]
mod codegen_terminal_v_render_tests;
