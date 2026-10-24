//! RenderIR → V hydrate 函数（Solid 细粒度绑定 + `dom.*` 宿主 API）。

use std::fmt::Write as _;

use crate::awsl::{
    BindingKind, RenderAttr, RenderAttrValue, RenderIfNode, RenderIr, RenderLoopNode, RenderModule, RenderNode, RenderNodeId, RenderRegionId,
    RenderTextSegment, ScriptBinding, SignalValueType,
    expr_deps::reactive_deps,
    expr_util::split_awsl_ternary,
    is_fragment_root,
    render_ir::{attr_value_source, region_nodes},
};

/// 将组件 RenderIR 生成为完整 V 模块（expr/evt 导出 + `awsl_hydrate_{route}` 或 `awsl_mount_{route}`）。
pub fn render_component_v(route: &str, bindings: &[ScriptBinding], script_prelude: &str, module: &RenderIr) -> String {
    if is_fragment_root(module) {
        return render_fragment_component_v(route, bindings, script_prelude, module);
    }
    let hydrate_name = hydrate_name_for_route(route);
    let mut aux = AuxExports::new(route);
    let mut hydrate_body = String::new();
    writeln!(hydrate_body, "    store_subscribe(\"{hydrate_name}_store\")").unwrap();
    if !script_prelude.trim().is_empty() {
        for line in script_prelude.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            writeln!(hydrate_body, "    {line}").unwrap();
        }
    }
    emit_memoized_bindings(bindings, &mut hydrate_body, route, &mut aux);
    let mut codegen = VRenderCodegen { route, module, bindings, aux: &mut aux, out: &mut hydrate_body, var_counter: 0, loop_stack: Vec::new() };
    let root = codegen.emit_roots();
    let mut out = String::new();
    aux.flush(&mut out);
    writeln!(out, "micro {hydrate_name}(): i32 {{").unwrap();
    out.push_str(&hydrate_body);
    writeln!(out, "    return {root}").unwrap();
    writeln!(out, "}}").unwrap();
    out
}

fn render_fragment_component_v(route: &str, bindings: &[ScriptBinding], script_prelude: &str, module: &RenderIr) -> String {
    let mount_name = mount_name_for_route(route);
    let mut aux = AuxExports::new(route);
    let mut mount_body = String::new();
    writeln!(mount_body, "    store_subscribe(\"{mount_name}_store\")").unwrap();
    if !script_prelude.trim().is_empty() {
        for line in script_prelude.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            writeln!(mount_body, "    {line}").unwrap();
        }
    }
    emit_memoized_bindings(bindings, &mut mount_body, route, &mut aux);
    let fragment_region = match module.roots.first().map(|&id| module.node(id)) {
        Some(RenderNode::Fragment(fragment)) => fragment.children,
        _ => RenderRegionId::EMPTY,
    };
    let mut codegen = VRenderCodegen { route, module, bindings, aux: &mut aux, out: &mut mount_body, var_counter: 0, loop_stack: Vec::new() };
    if fragment_region != RenderRegionId::EMPTY {
        codegen.emit_region_into(fragment_region, "container");
    }
    else {
        codegen.emit_roots_into("container");
    }
    let mut out = String::new();
    aux.flush(&mut out);
    writeln!(out, "micro {mount_name}(container: i32): i32 {{").unwrap();
    out.push_str(&mount_body);
    writeln!(out, "    return container").unwrap();
    writeln!(out, "}}").unwrap();
    out
}

struct AuxExports {
    route: String,
    items: Vec<String>,
}

impl AuxExports {
    fn new(route: &str) -> Self {
        Self { route: route.to_string(), items: Vec::new() }
    }

    fn push(&mut self, item: String) {
        self.items.push(item);
    }

    fn flush(&mut self, out: &mut String) {
        for item in self.items.drain(..) {
            out.push_str(&item);
            if !item.ends_with('\n') {
                out.push('\n');
            }
        }
    }
}

#[derive(Debug, Clone)]
struct LoopFrame {
    item_var: String,
    index_var: String,
    items_expr: String,
}

struct VRenderCodegen<'a> {
    route: &'a str,
    module: &'a RenderModule,
    bindings: &'a [ScriptBinding],
    aux: &'a mut AuxExports,
    out: &'a mut String,
    var_counter: u32,
    loop_stack: Vec<LoopFrame>,
}

impl<'a> VRenderCodegen<'a> {
    fn next_var(&mut self) -> String {
        let name = format!("h{}", self.var_counter);
        self.var_counter += 1;
        name
    }

    fn next_export_id(&mut self, kind: &str) -> String {
        let id = self.aux.items.len();
        format!("awsl_{kind}_{}_{id}", sanitize_export(self.route))
    }

    fn dep_args(&self, expr: &str) -> (String, String) {
        let mut ids: Vec<String> = reactive_deps(expr, self.bindings)
            .iter()
            .filter_map(|name| self.bindings.iter().find(|b| b.name == *name))
            .map(|b| b.sig_id_expr())
            .collect();
        if (expr.contains("get_") || expr.contains("count_") || expr.contains("todos(")) && ids.len() < 2 {
            ids.push("-2".into());
        }
        let dep1 = ids.first().cloned().unwrap_or_else(|| "-1".into());
        let dep2 = ids.get(1).cloned().unwrap_or_else(|| "-1".into());
        (dep1, dep2)
    }

    fn rewrite_expr(&self, expr: &str) -> String {
        rewrite_expr_with_loops(expr, self.bindings, &self.loop_stack)
    }

    fn register_utf8_expr(&mut self, expr: &str) -> String {
        let name = self.next_export_id("expr");
        let body = rewrite_utf8_expr_body(expr, self.bindings, &self.loop_stack);
        self.aux.push(format!("micro {name}(): utf8 {{\n    return {body}\n}}"));
        name
    }

    fn register_bool_expr(&mut self, expr: &str) -> String {
        let name = self.next_export_id("cond");
        let rewritten = self.rewrite_expr(expr);
        self.aux.push(format!("micro {name}(): bool {{\n    return {rewritten}\n}}"));
        name
    }

    fn register_event(&mut self, handler: &str) -> (String, Option<EventArgKind>) {
        let name = self.next_export_id("evt");
        let handler = strip_handler_wrapper(handler);
        match parse_event_handler(&handler) {
            EventHandler::Simple(fn_name) => {
                self.aux.push(format!(
                    "micro {name}() {{\n    rx_batch_begin()\n    awsl_call_{}()\n    store_bump()\n    rx_batch_end()\n}}",
                    sanitize_export(&fn_name)
                ));
                (name, None)
            }
            EventHandler::Call { fn_name, arg } => {
                let (body, kind) = self.emit_call_event_body(&name, &fn_name, &arg);
                self.aux.push(body);
                (name, kind)
            }
            EventHandler::Unknown => {
                self.aux.push(format!("micro {name}() {{\n    rx_batch_begin()\n    store_bump()\n    rx_batch_end()\n}}"));
                (name, None)
            }
        }
    }

    fn emit_call_event_body(&self, evt_name: &str, fn_name: &str, arg: &str) -> (String, Option<EventArgKind>) {
        let fn_export = sanitize_export(fn_name);
        let arg = arg.trim();
        if let Some(frame) = self.loop_stack.iter().rev().find(|f| f.item_var == arg) {
            let items = &frame.items_expr;
            let body = format!(
                "micro {evt_name}(index: i32) {{\n    rx_batch_begin()\n    awsl_call_{fn_export}({items}[index])\n    store_bump()\n    rx_batch_end()\n}}"
            );
            return (body, Some(EventArgKind::I32("index".into())));
        }
        if let Some((item_var, field)) = split_field_access(arg) {
            if self.loop_stack.iter().any(|f| f.item_var == item_var) {
                let body = format!(
                    "micro {evt_name}(arg: utf8) {{\n    rx_batch_begin()\n    awsl_call_{fn_export}(arg)\n    store_bump()\n    rx_batch_end()\n}}"
                );
                let bind_expr = format!("{item_var}.{field}");
                return (body, Some(EventArgKind::Utf8(bind_expr)));
            }
        }
        let rewritten = self.rewrite_expr(arg);
        let body = format!(
            "micro {evt_name}() {{\n    rx_batch_begin()\n    awsl_call_{fn_export}({rewritten})\n    store_bump()\n    rx_batch_end()\n}}"
        );
        (body, None)
    }

    fn emit_roots(&mut self) -> String {
        self.emit_node_ids(&self.module.roots)
    }

    fn emit_roots_into(&mut self, container: &str) {
        for &node_id in &self.module.roots {
            let child = self.emit_node(node_id);
            writeln!(self.out, "    dom_append({container}, {child})").unwrap();
        }
    }

    fn emit_node_ids(&mut self, node_ids: &[RenderNodeId]) -> String {
        if node_ids.is_empty() {
            return self.emit_text_v("");
        }
        if node_ids.len() == 1 {
            return self.emit_node(node_ids[0]);
        }
        let frag = self.next_var();
        writeln!(self.out, "    let {frag} = dom_create_element(\"div\")").unwrap();
        for &node_id in node_ids {
            let child = self.emit_node(node_id);
            writeln!(self.out, "    dom_append({frag}, {child})").unwrap();
        }
        frag
    }

    fn emit_region_into(&mut self, region: RenderRegionId, container: &str) {
        for &node_id in region_nodes(self.module, region) {
            let child = self.emit_node(node_id);
            writeln!(self.out, "    dom_append({container}, {child})").unwrap();
        }
    }

    fn emit_node(&mut self, node_id: RenderNodeId) -> String {
        match self.module.node(node_id) {
            RenderNode::Fragment(fragment) => {
                let children = region_nodes(self.module, fragment.children);
                if children.len() == 1 {
                    return self.emit_node(children[0]);
                }
                let frag = self.next_var();
                writeln!(self.out, "    let {frag} = dom_create_element(\"div\")").unwrap();
                for &child_id in children {
                    let child = self.emit_node(child_id);
                    writeln!(self.out, "    dom_append({frag}, {child})").unwrap();
                }
                frag
            }
            RenderNode::Element(element) => {
                if let Some(intrinsic) = map_awsl_component_to_host(&element.tag) {
                    return self.emit_view_tag(intrinsic, &element.attrs, element.children);
                }
                self.emit_view_tag(&element.tag, &element.attrs, element.children)
            }
            RenderNode::Component(component) => self.emit_component_call(&component.tag, &component.attrs, component.children),
            RenderNode::Text(text) => self.emit_text_segments(&text.segments),
            RenderNode::If(render_if) => self.emit_if(render_if),
            RenderNode::Loop(render_loop) => self.emit_loop(render_loop),
        }
    }

    fn emit_if(&mut self, render_if: &RenderIfNode) -> String {
        let container = self.next_var();
        writeln!(self.out, "    let {container} = dom_create_element(\"div\")").unwrap();
        let condition = self.module.expr_source(render_if.condition);
        let cond_export = self.register_bool_expr(condition);
        let mount_export = self.register_mount_export(render_if.then_region);
        let (dep1, dep2) = self.dep_args(condition);
        writeln!(self.out, "    rx_bind_if({container}, \"{cond_export}\", \"{mount_export}\", {dep1}, {dep2})").unwrap();
        container
    }

    fn emit_loop(&mut self, render_loop: &RenderLoopNode) -> String {
        let container = self.next_var();
        writeln!(self.out, "    let {container} = dom_create_element(\"div\")").unwrap();
        let items_source = self.module.expr_source(render_loop.items);
        let items_rewrite = self.rewrite_expr(items_source);
        let items_export = self.register_list_expr(items_source);
        let mount_export = self.register_loop_mount_export(render_loop, &items_rewrite);
        let key_export = render_loop
            .key
            .map(|key_id| self.register_utf8_expr(self.module.expr_source(key_id)))
            .unwrap_or_else(|| "awsl_key_none".to_string());
        let (dep1, mut dep2) = self.dep_args(items_source);
        if region_uses_reactive(self.module, render_loop.body_region, self.bindings) && dep2 == "-1" {
            dep2 = "-2".into();
        }
        writeln!(self.out, "    rx_bind_loop({container}, \"{items_export}\", \"{mount_export}\", \"{key_export}\", {dep1}, {dep2})").unwrap();
        container
    }

    fn register_list_expr(&mut self, expr: &str) -> String {
        let name = self.next_export_id("items");
        let rewritten = self.rewrite_expr(expr);
        self.aux.push(format!("micro {name}(): list {{\n    return {rewritten}\n}}"));
        name
    }

    fn register_mount_export(&mut self, region: RenderRegionId) -> String {
        let name = self.next_export_id("mount");
        let mut body = String::new();
        let mut inner = VRenderCodegen {
            route: self.route,
            module: self.module,
            bindings: self.bindings,
            aux: self.aux,
            out: &mut body,
            var_counter: self.var_counter,
            loop_stack: self.loop_stack.clone(),
        };
        let child_ids = region_nodes(self.module, region);
        if child_ids.len() == 1 {
            if let RenderNode::Fragment(fragment) = self.module.node(child_ids[0]) {
                inner.emit_region_into(fragment.children, "container");
                self.var_counter = inner.var_counter;
                self.aux.push(format!("micro {name}(container: i32): i32 {{\n{body}    return container\n}}"));
                return name;
            }
        }
        let root = inner.emit_node_ids(child_ids);
        self.var_counter = inner.var_counter;
        writeln!(body, "    let mounted = {root}").unwrap();
        writeln!(body, "    dom_append(container, mounted)").unwrap();
        writeln!(body, "    return mounted").unwrap();
        self.aux.push(format!("micro {name}(container: i32): i32 {{\n{body}}}"));
        name
    }

    fn register_loop_mount_export(&mut self, render_loop: &RenderLoopNode, items_rewrite: &str) -> String {
        let name = self.next_export_id("loop_mount");
        let frame = LoopFrame {
            item_var: render_loop.item_var.clone(),
            index_var: render_loop.index_var.clone(),
            items_expr: items_rewrite.to_string(),
        };
        let mut mount_body = String::new();
        let mut stack = self.loop_stack.clone();
        stack.push(frame.clone());
        let mut inner = VRenderCodegen {
            route: self.route,
            module: self.module,
            bindings: self.bindings,
            aux: self.aux,
            out: &mut mount_body,
            var_counter: self.var_counter,
            loop_stack: stack,
        };
        let child = inner.emit_node_ids(region_nodes(self.module, render_loop.body_region));
        self.var_counter = inner.var_counter;
        let item_var = &frame.item_var;
        self.aux.push(format!(
            "micro {name}(container: i32, index: i32): i32 {{\n    let {item_var} = {items_rewrite}[index]\n{mount_body}    let child = {child}\n    dom_append(container, child)\n    return child\n}}"
        ));
        name
    }

    fn emit_view_tag(&mut self, tag: &str, attrs: &[RenderAttr], children: RenderRegionId) -> String {
        let host_tag = match tag {
            "Text" | "text" => "span",
            "Column" | "Box" | "Row" | "Flex" | "column" | "box" | "row" | "flex" => "div",
            "Button" | "button" => "button",
            other => other,
        };
        let var = self.next_var();
        writeln!(self.out, "    let {var} = dom_create_element(\"{host_tag}\")").unwrap();
        for attr in attrs {
            self.emit_attr(&var, attr);
        }
        for &child_id in region_nodes(self.module, children) {
            let child_var = self.emit_node(child_id);
            writeln!(self.out, "    dom_append({var}, {child_var})").unwrap();
        }
        var
    }

    fn emit_component_call(&mut self, tag: &str, attrs: &[RenderAttr], children: RenderRegionId) -> String {
        let export = hydrate_name_for_route(tag);
        let var = self.next_var();
        writeln!(self.out, "    let {var} = {export}()").unwrap();
        for attr in attrs {
            if attr.is_prop {
                self.emit_prop_bind(tag, attr);
            }
            else if !attr.is_event {
                self.emit_attr(&var, attr);
            }
        }
        for attr in attrs {
            if attr.is_event {
                self.emit_event_attr(&var, attr);
            }
        }
        for &child_id in region_nodes(self.module, children) {
            let child_var = self.emit_node(child_id);
            writeln!(self.out, "    dom_append({var}, {child_var})").unwrap();
        }
        var
    }

    fn register_utf8_literal(&mut self, text: &str) -> String {
        let name = self.next_export_id("expr");
        let escaped = escape_v_string(text);
        self.aux.push(format!("micro {name}(): utf8 {{\n    return \"{escaped}\"\n}}"));
        name
    }

    fn emit_prop_bind(&mut self, component: &str, attr: &RenderAttr) {
        let sig_export = format!("awsl_sig_{}_{}", sanitize_export(component), sanitize_export(&attr.name));
        let (expr_export, dep_source): (String, String) = match &attr.value {
            RenderAttrValue::Static(text) => (self.register_utf8_literal(text), String::new()),
            RenderAttrValue::Expr(expr_id) => {
                let source = self.module.expr_source(*expr_id).to_string();
                (self.register_utf8_expr(&source), source)
            }
            RenderAttrValue::Template(segments) => {
                let expr = segments
                    .iter()
                    .map(|segment| match segment {
                        RenderTextSegment::Static(text) => format!("\"{}\"", escape_v_string(text)),
                        RenderTextSegment::Expr(expr_id) => {
                            rewrite_utf8_expr_body(self.module.expr_source(*expr_id), self.bindings, &self.loop_stack)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" + ");
                let export = self.register_utf8_expr(&expr);
                (export, expr)
            }
        };
        let (dep1, dep2) = self.dep_args(&dep_source);
        writeln!(self.out, "    rx_bind_prop_utf8({sig_export}(), \"{expr_export}\", {dep1}, {dep2})").unwrap();
    }

    fn emit_attr(&mut self, handle: &str, attr: &RenderAttr) {
        if attr.is_event {
            self.emit_event_attr(handle, attr);
            return;
        }
        match &attr.value {
            RenderAttrValue::Static(text) => {
                writeln!(self.out, "    dom_set_attr({handle}, \"{}\", \"{text}\")", attr.name).unwrap();
            }
            RenderAttrValue::Expr(expr_id) => self.emit_dynamic_attr(handle, &attr.name, self.module.expr_source(*expr_id)),
            RenderAttrValue::Template(segments) => {
                let expr = segments
                    .iter()
                    .map(|segment| match segment {
                        RenderTextSegment::Static(text) => format!("\"{}\"", escape_v_string(text)),
                        RenderTextSegment::Expr(expr_id) => {
                            rewrite_utf8_expr_body(self.module.expr_source(*expr_id), self.bindings, &self.loop_stack)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" + ");
                if !self.loop_stack.is_empty() {
                    writeln!(self.out, "    dom_set_attr({handle}, \"{}\", {expr})", attr.name).unwrap();
                }
                else {
                    self.emit_dynamic_attr(handle, &attr.name, &expr);
                }
            }
        }
    }

    fn emit_dynamic_attr(&mut self, handle: &str, name: &str, expr: &str) {
        if !self.loop_stack.is_empty() {
            let body = rewrite_utf8_expr_body(expr, self.bindings, &self.loop_stack);
            writeln!(self.out, "    dom_set_attr({handle}, \"{name}\", {body})").unwrap();
            return;
        }
        let export = self.register_utf8_expr(expr);
        let (dep1, dep2) = self.dep_args(expr);
        if name == "class" {
            writeln!(self.out, "    rx_bind_class_utf8({handle}, \"{export}\", {dep1}, {dep2})").unwrap();
        }
        else {
            writeln!(self.out, "    rx_bind_attr_utf8({handle}, \"{name}\", \"{export}\", {dep1}, {dep2})").unwrap();
        }
    }

    fn emit_event_attr(&mut self, handle: &str, attr: &RenderAttr) {
        let handler = attr_value_source(self.module, &attr.value);
        let event = attr.name.strip_prefix("on:").or_else(|| attr.name.strip_prefix('@')).unwrap_or("click");
        let (export, arg_kind) = self.register_event(&handler);
        match arg_kind {
            Some(EventArgKind::Utf8(bind_expr)) => {
                let rewritten = self.rewrite_expr(&bind_expr);
                writeln!(self.out, "    dom_add_event_export_utf8({handle}, \"{event}\", \"{export}\", {rewritten})").unwrap();
            }
            Some(EventArgKind::I32(bind_expr)) => {
                writeln!(self.out, "    dom_add_event_export_i32({handle}, \"{event}\", \"{export}\", {bind_expr})").unwrap();
            }
            None => {
                writeln!(self.out, "    dom_add_event_export({handle}, \"{event}\", \"{export}\")").unwrap();
            }
        }
    }

    fn emit_text_segments(&mut self, segments: &[RenderTextSegment]) -> String {
        if segments.len() == 1 {
            return match &segments[0] {
                RenderTextSegment::Static(text) => self.emit_text_v(text),
                RenderTextSegment::Expr(expr_id) => self.emit_dynamic_text(self.module.expr_source(*expr_id)),
            };
        }
        let var = self.next_var();
        writeln!(self.out, "    let {var} = dom_create_element(\"span\")").unwrap();
        for segment in segments {
            let child = match segment {
                RenderTextSegment::Static(text) => self.emit_text_v(text),
                RenderTextSegment::Expr(expr_id) => self.emit_dynamic_text(self.module.expr_source(*expr_id)),
            };
            writeln!(self.out, "    dom_append({var}, {child})").unwrap();
        }
        var
    }

    fn emit_dynamic_text(&mut self, expr: &str) -> String {
        let var = self.next_var();
        if !self.loop_stack.is_empty() {
            let body = rewrite_utf8_expr_body(expr, self.bindings, &self.loop_stack);
            writeln!(self.out, "    let {var} = dom_create_text({body})").unwrap();
            return var;
        }
        writeln!(self.out, "    let {var} = dom_create_text(\"\")").unwrap();
        let export = self.register_utf8_expr(expr);
        let (dep1, dep2) = self.dep_args(expr);
        writeln!(self.out, "    rx_bind_text_utf8({var}, \"{export}\", {dep1}, {dep2})").unwrap();
        var
    }

    fn emit_text_v(&mut self, text: &str) -> String {
        let var = self.next_var();
        let escaped = escape_v_string(text);
        writeln!(self.out, "    let {var} = dom_create_text(\"{escaped}\")").unwrap();
        var
    }
}

fn expand_awsl_fstring(expr: &str) -> Option<String> {
    let trimmed = expr.trim();
    let rest = trimmed.strip_prefix('f')?;
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    if rest.len() < 2 || !rest.ends_with(quote) {
        return None;
    }
    let inner = &rest[1..rest.len() - 1];
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            if !literal.is_empty() {
                parts.push(format!("\"{}\"", escape_v_string(&literal)));
                literal.clear();
            }
            let mut depth = 1usize;
            let mut expr_body = String::new();
            while let Some(c) = chars.next() {
                if c == '{' {
                    depth += 1;
                    expr_body.push(c);
                }
                else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    expr_body.push(c);
                }
                else {
                    expr_body.push(c);
                }
            }
            parts.push(expr_body.trim().to_string());
            continue;
        }
        literal.push(ch);
    }
    if !literal.is_empty() {
        parts.push(format!("\"{}\"", escape_v_string(&literal)));
    }
    if parts.is_empty() {
        return Some("\"\"".into());
    }
    Some(parts.join(" + "))
}

fn rewrite_utf8_expr_body(expr: &str, bindings: &[ScriptBinding], loops: &[LoopFrame]) -> String {
    let trimmed = expr.trim();
    if let Some(expanded) = expand_awsl_fstring(trimmed) {
        return rewrite_utf8_expr_body(&expanded, bindings, loops);
    }
    if let Some((cond, then_arm, else_arm)) = split_awsl_ternary(trimmed) {
        return format!(
            "(if {} {{ {} }} else {{ {} }})",
            rewrite_expr_with_loops(&cond, bindings, loops),
            rewrite_utf8_expr_body(&then_arm, bindings, loops),
            rewrite_utf8_expr_body(&else_arm, bindings, loops)
        );
    }
    for binding in bindings {
        if binding.reactive && trimmed == binding.name {
            return match binding.value_type {
                SignalValueType::Utf8 => binding.read_expr(),
                SignalValueType::I32 | SignalValueType::Bool => format!("utf8({})", binding.read_expr()),
            };
        }
    }
    let mut out = rewrite_expr_with_loops(trimmed, bindings, loops);
    if out.starts_with('"') && out.ends_with('"') && out.len() >= 2 {
        return out;
    }
    if out.parse::<i32>().is_ok() {
        return format!("utf8({out})");
    }
    if out == "true" || out == "false" {
        return format!("\"{out}\"");
    }
    if is_simple_identifier(&out) {
        return out;
    }
    if is_loop_field_access(&out, loops) {
        return format!("utf8({out})");
    }
    if out.contains('+') {
        return out.split('+').map(|part| rewrite_utf8_expr_body(part.trim(), bindings, loops)).collect::<Vec<_>>().join(" + ");
    }
    if out.contains('(') || out.contains('.') {
        return format!("utf8({})", normalize_awsl_js_ops(&out));
    }
    awsl_expr_to_v(&out)
}

fn rewrite_expr_with_loops(expr: &str, bindings: &[ScriptBinding], loops: &[LoopFrame]) -> String {
    let mut out = normalize_js_string_literals(strip_expr_braces(expr.trim()));
    for binding in bindings.iter().filter(|b| b.reactive) {
        if loops.iter().any(|f| f.item_var == binding.name || f.index_var == binding.name) {
            continue;
        }
        out = replace_ident(&out, &binding.name, &binding.read_expr());
    }
    normalize_awsl_js_ops(&out)
}

fn replace_ident(expr: &str, name: &str, replacement: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = expr.chars().collect();
    let name_chars: Vec<char> = name.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i..].starts_with(&name_chars) {
            let before_ok = i == 0 || !is_ident_char(chars[i - 1]);
            let after = i + name_chars.len();
            let after_ok = after >= chars.len() || !is_ident_char(chars[after]);
            if before_ok && after_ok {
                out.push_str(replacement);
                i = after;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn is_loop_field_access(expr: &str, loops: &[LoopFrame]) -> bool {
    if let Some((head, _)) = expr.split_once('.') {
        return loops.iter().any(|f| f.item_var == head);
    }
    loops.iter().any(|f| f.item_var == expr || f.index_var == expr)
}

fn strip_expr_braces(expr: &str) -> &str {
    let trimmed = expr.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') && trimmed.len() >= 2 { trimmed[1..trimmed.len() - 1].trim() } else { trimmed }
}

fn is_simple_identifier(expr: &str) -> bool {
    !expr.is_empty() && expr.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[derive(Debug, Clone)]
enum EventArgKind {
    Utf8(String),
    I32(String),
}

#[derive(Debug, Clone)]
enum EventHandler {
    Simple(String),
    Call { fn_name: String, arg: String },
    Unknown,
}

fn strip_handler_wrapper(handler: &str) -> String {
    let mut handler = handler.trim().trim_matches('"').trim_matches('\'').to_string();
    if let Some(idx) = handler.find("=>") {
        let left = handler[..idx].trim();
        if left == "()" || left == "( )" {
            handler = handler[idx + 2..].trim().to_string();
        }
    }
    handler
}

fn parse_event_handler(handler: &str) -> EventHandler {
    let handler = handler.trim();
    if is_simple_identifier(handler) {
        return EventHandler::Simple(handler.to_string());
    }
    if let Some(open) = handler.find('(') {
        if handler.ends_with(')') {
            let fn_name = handler[..open].trim();
            let arg = handler[open + 1..handler.len() - 1].trim();
            if is_simple_identifier(fn_name) && !arg.is_empty() && !arg.contains(',') {
                return EventHandler::Call { fn_name: fn_name.to_string(), arg: arg.to_string() };
            }
        }
    }
    EventHandler::Unknown
}

fn split_field_access(expr: &str) -> Option<(String, String)> {
    let (head, tail) = expr.split_once('.')?;
    if is_simple_identifier(head) && is_simple_identifier(tail) {
        return Some((head.to_string(), tail.to_string()));
    }
    None
}

fn region_uses_reactive(module: &RenderModule, region: RenderRegionId, bindings: &[ScriptBinding]) -> bool {
    for &node_id in region_nodes(module, region) {
        if node_uses_reactive(module, node_id, bindings) {
            return true;
        }
    }
    false
}

fn node_uses_reactive(module: &RenderModule, node_id: RenderNodeId, bindings: &[ScriptBinding]) -> bool {
    match module.node(node_id) {
        RenderNode::Element(element) => {
            for attr in &element.attrs {
                let src = attr_value_source(module, &attr.value);
                if expr_uses_reactive(&src, bindings) {
                    return true;
                }
            }
            region_uses_reactive(module, element.children, bindings)
        }
        RenderNode::Component(component) => {
            for attr in &component.attrs {
                let src = attr_value_source(module, &attr.value);
                if expr_uses_reactive(&src, bindings) {
                    return true;
                }
            }
            region_uses_reactive(module, component.children, bindings)
        }
        RenderNode::Text(text) => text.segments.iter().any(|segment| match segment {
            RenderTextSegment::Expr(expr_id) => expr_uses_reactive(module.expr_source(*expr_id), bindings),
            RenderTextSegment::Static(_) => false,
        }),
        RenderNode::If(render_if) => {
            expr_uses_reactive(module.expr_source(render_if.condition), bindings)
                || region_uses_reactive(module, render_if.then_region, bindings)
                || region_uses_reactive(module, render_if.else_region, bindings)
        }
        RenderNode::Loop(render_loop) => {
            expr_uses_reactive(module.expr_source(render_loop.items), bindings)
                || region_uses_reactive(module, render_loop.body_region, bindings)
        }
        RenderNode::Fragment(fragment) => region_uses_reactive(module, fragment.children, bindings),
    }
}

fn expr_uses_reactive(expr: &str, bindings: &[ScriptBinding]) -> bool {
    bindings.iter().any(|b| b.reactive && expr.contains(&b.name))
}

/// 路由的 hydrate 导出名（`awsl_hydrate_<route>`）。
pub fn hydrate_name_for_route(route_name: &str) -> String {
    format!("awsl_hydrate_{}", sanitize_export(route_name))
}

/// 路由的 mount 导出名（`awsl_mount_<route>`）。
pub fn mount_name_for_route(route_name: &str) -> String {
    format!("awsl_mount_{}", sanitize_export(route_name))
}

/// 路由的导出名（与 hydrate 名一致）。
pub fn export_name_for_route(route_name: &str) -> String {
    hydrate_name_for_route(route_name)
}

/// 路由的 mount 导出名。
pub fn export_mount_name_for_route(route_name: &str) -> String {
    mount_name_for_route(route_name)
}

fn sanitize_export(name: &str) -> String {
    name.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '_' { ch } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_ascii_lowercase()
}

fn map_awsl_component_to_host(tag: &str) -> Option<&'static str> {
    match tag {
        "Link" => Some("a"),
        "Badge" => Some("span"),
        "Avatar" => Some("span"),
        "Input" => Some("input"),
        "Button" => Some("button"),
        _ => None,
    }
}

fn normalize_awsl_js_ops(expr: &str) -> String {
    if expr.is_empty() {
        return "true".into();
    }
    normalize_js_string_literals(expr).replace("===", "==").replace("!==", "!=")
}

fn normalize_js_string_literals(expr: &str) -> String {
    let mut out = String::new();
    let mut chars = expr.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\'' {
            let mut inner = String::new();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    if let Some(next) = chars.next() {
                        inner.push(next);
                    }
                }
                else if c == '\'' {
                    break;
                }
                else {
                    inner.push(c);
                }
            }
            out.push('"');
            out.push_str(&escape_v_string(&inner));
            out.push('"');
        }
        else if ch == '"' {
            out.push(ch);
            while let Some(c) = chars.next() {
                out.push(c);
                if c == '\\' {
                    if let Some(next) = chars.next() {
                        out.push(next);
                    }
                }
                else if c == '"' {
                    break;
                }
            }
        }
        else {
            out.push(ch);
        }
    }
    out
}

fn awsl_expr_to_v(expr: &str) -> String {
    let trimmed = expr.trim();
    if let Some(expanded) = expand_awsl_fstring(trimmed) {
        return awsl_expr_to_v(&expanded);
    }
    let trimmed = normalize_js_string_literals(trimmed);
    if trimmed.is_empty() {
        return "\"\"".into();
    }
    if let Some((cond, then_arm, else_arm)) = split_awsl_ternary(&trimmed) {
        return format!("if {} {{ {} }} else {{ {} }}", awsl_expr_to_v(&cond), awsl_expr_to_v(&then_arm), awsl_expr_to_v(&else_arm));
    }
    if (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2)
    {
        let inner = &trimmed[1..trimmed.len() - 1];
        return format!("\"{}\"", escape_v_string(inner));
    }
    if trimmed.parse::<i32>().is_ok() {
        return trimmed;
    }
    if trimmed == "true" || trimmed == "false" {
        return format!("\"{trimmed}\"");
    }
    if is_simple_identifier(&trimmed) {
        return trimmed;
    }
    format!("utf8({})", normalize_awsl_js_ops(&trimmed))
}

fn emit_memoized_bindings(bindings: &[ScriptBinding], body: &mut String, route: &str, aux: &mut AuxExports) {
    let mut memo_id = 0i32;
    for binding in bindings {
        if binding.kind != BindingKind::Memoized {
            continue;
        }
        memo_id += 1;
        let export = format!("awsl_memo_{}_{}", sanitize_export(route), sanitize_export(&binding.name));
        aux.push(format!(
            "micro {export}(): {} {{\n    return {}\n}}",
            match binding.value_type {
                SignalValueType::I32 => "i32",
                SignalValueType::Bool => "bool",
                SignalValueType::Utf8 => "utf8",
            },
            rewrite_utf8_expr_body(&binding.init_expr, bindings, &[])
        ));
        let deps = reactive_deps(&binding.init_expr, bindings);
        let dep1 =
            deps.first().and_then(|name| bindings.iter().find(|b| b.name == *name)).map(|b| b.sig_id_expr()).unwrap_or_else(|| "-1".into());
        let dep2 =
            deps.get(1).and_then(|name| bindings.iter().find(|b| b.name == *name)).map(|b| b.sig_id_expr()).unwrap_or_else(|| "-1".into());
        let call = match binding.value_type {
            SignalValueType::I32 => format!("rx_memo_i32({memo_id}, \"{export}\", {dep1}, {dep2})"),
            SignalValueType::Utf8 => format!("rx_memo_utf8({memo_id}, \"{export}\", {dep1}, {dep2})"),
            SignalValueType::Bool => format!("rx_memo_utf8({memo_id}, \"{export}\", {dep1}, {dep2})"),
        };
        writeln!(body, "    let {} = {call}", binding.name).unwrap();
    }
}

fn escape_v_string(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"").replace('{', "\\{").replace('}', "\\}")
}
