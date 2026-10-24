//! AWSL 前端降级：AST → synthetic V + RenderIR。

use oak_awsl::{
    AwslAttributeValue, AwslDirectiveKind, AwslElement, AwslRoot, AwslTemplateNode, AwslTextPart, ComponentAbi, widget_name_from_stem,
};

use super::abi_index::extract_abi_for_script;

use super::render_ir::{
    SurfaceAttr, SurfaceAttrValue, SurfaceIf, SurfaceIr, SurfaceLoop, SurfaceNode, SurfaceTextPart, TemplateNodeKind, ThemeRegistryOptions,
    canonicalize_surface, is_intrinsic_tag, run_default_passes,
};

/// 降级选项。
#[derive(Debug, Clone)]
pub struct LoweringOptions {
    /// AWSL 严格模式。
    pub strict_mode: bool,
    /// 默认 island 类型。
    pub default_island: String,
    /// 默认 hydrate 策略。
    pub default_strategy: String,
}

impl Default for LoweringOptions {
    fn default() -> Self {
        Self { strict_mode: true, default_island: "hydrated".into(), default_strategy: "visible".into() }
    }
}

/// 降级后的组件产物。
#[derive(Debug, Clone)]
pub struct LoweredComponent {
    /// 组件名（snake_case，与 `<widget>` / 文件 stem 一致）。
    pub name: String,
    /// 路由/文件名（来自 `.awsl` 文件 stem，如 `index` 或 `[slug]`）。
    pub route_name: String,
    /// 进入 Valkyrie 语义主线的 synthetic V 源码。
    pub synthetic_v: String,
    /// 交付阶段 canonical RenderIR。
    pub render_ir: super::render_ir::RenderIr,
    /// 样式块原文。
    pub style: Option<String>,
    /// script 块顶层绑定（变量名、初始表达式、是否响应式 `let mut`）。
    pub script_bindings: Vec<ScriptBinding>,
    /// Extracted component ABI.
    pub component_abi: ComponentAbi,
    /// ABI semantic issues from script extraction.
    pub abi_issues: Vec<oak_awsl::AbiIssue>,
    /// island 类型。
    pub island_type: String,
    /// hydrate 策略。
    pub hydrate_strategy: String,
    /// 源文件路径（用于 source map）。
    pub source_path: String,
    /// `<script>` 块原文（供 StyleCollector 静态扫描 `StyleCollector.push`）。
    pub script_source: Option<String>,
}

/// `<script>` 顶层绑定分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    /// `[property] let` component input.
    Property {
        /// Required when no default initializer is present.
        required: bool,
    },
    /// `let mut` internal reactive state.
    ReactiveState,
    /// `let = expr` derived binding.
    Derived,
    /// `[memoize] let = expr` cached derived binding.
    Memoized,
    /// Non-ABI constant binding.
    LocalConst,
}

/// `<script>` 顶层 `let` / `let mut` 绑定。
#[derive(Debug, Clone)]
pub struct ScriptBinding {
    /// 变量名。
    pub name: String,
    /// 初始表达式。
    pub init_expr: String,
    /// Binding semantic kind.
    pub kind: BindingKind,
    /// `let mut` / reactive state 为 `true`。
    pub reactive: bool,
    /// 对应信号槽变量名（`filter_sig`）。
    pub sig_var: String,
    /// 信号值类型。
    pub value_type: SignalValueType,
}

/// 响应式信号值类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalValueType {
    /// `i32`
    I32,
    /// `utf8`
    Utf8,
    /// `bool`
    Bool,
}

/// 将 AWSL 根节点降级。
pub fn lower_component(root: &AwslRoot, component_name: &str, source_path: &str, options: &LoweringOptions) -> LoweredComponent {
    let name = widget_name_from_stem(component_name);
    let surface = wrap_widget_roots_in_fragment(lower_template_nodes(&root.template));
    let (component_abi, abi_issues) = root
        .script
        .as_deref()
        .map(|script| extract_abi_for_script(script, &name))
        .unwrap_or_else(|| (ComponentAbi { widget_name: name.clone(), ..Default::default() }, Vec::new()));
    let mut script_bindings = abi_to_script_bindings(&component_abi);
    finalize_signal_slots(&mut script_bindings);
    let mut render_ir = canonicalize_surface(surface, &script_bindings);
    run_default_passes(&mut render_ir, ThemeRegistryOptions { single_theme: None, single_mode: None });
    let synthetic_v = generate_synthetic_v(&name, root, &script_bindings);
    LoweredComponent {
        name,
        route_name: component_name.to_string(),
        synthetic_v,
        render_ir,
        style: root.style.clone(),
        script_bindings,
        component_abi,
        abi_issues,
        island_type: options.default_island.clone(),
        hydrate_strategy: options.default_strategy.clone(),
        source_path: source_path.to_string(),
        script_source: root.script.clone(),
    }
}

fn generate_synthetic_v(name: &str, root: &AwslRoot, bindings: &[ScriptBinding]) -> String {
    let Some(script) = &root.script
    else {
        return String::new();
    };
    let decls = rewrite_script_decls_for_signals(script, bindings);
    if decls.trim().is_empty() {
        return String::new();
    }
    let mut out = format!("# awsl script decls: {name}\n");
    out.push_str(&decls);
    out.push('\n');
    out
}

fn finalize_signal_slots(bindings: &mut [ScriptBinding]) {
    for binding in bindings.iter_mut() {
        if binding.reactive {
            binding.sig_var = format!("{}_sig", binding.name);
        }
    }
}

fn rewrite_script_decls_for_signals(script: &str, bindings: &[ScriptBinding]) -> String {
    let lines: Vec<&str> = script.lines().collect();
    let mut out = String::new();
    let mut index = 0usize;
    while index < lines.len() {
        let trimmed = lines[index].trim();
        if trimmed.starts_with("micro ") || trimmed.starts_with("class ") || trimmed.starts_with("trait ") {
            let (block, next) = take_braced_block(&lines, index);
            out.push_str(&rewrite_micro_body_for_signals(&block, bindings));
            if !out.ends_with('\n') {
                out.push('\n');
            }
            index = next;
            continue;
        }
        if trimmed.starts_with("let ") {
            index += 1;
            continue;
        }
        index += 1;
    }
    out
}

fn rewrite_micro_body_for_signals(block: &str, bindings: &[ScriptBinding]) -> String {
    let mut out = String::new();
    for line in block.lines() {
        out.push_str(&rewrite_micro_source_line(line, bindings));
        out.push('\n');
    }
    out
}

fn rewrite_micro_source_line(line: &str, bindings: &[ScriptBinding]) -> String {
    if let Some(open_brace) = line.find('{') {
        if let Some(close_brace) = line.rfind('}') {
            if close_brace > open_brace {
                let header = &line[..open_brace + 1];
                let body = line[open_brace + 1..close_brace].trim();
                let footer = &line[close_brace..];
                if !body.is_empty() {
                    if let Some(rewritten) = rewrite_signal_statement(body, bindings) {
                        return format!("{header}{rewritten}{footer}");
                    }
                }
            }
        }
    }
    let trimmed = line.trim();
    if let Some(rewritten) = rewrite_signal_statement(trimmed, bindings) {
        let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
        format!("{indent}{rewritten}")
    }
    else {
        substitute_reactive_reads_in_line(line, bindings)
    }
}

fn rewrite_signal_statement(statement: &str, bindings: &[ScriptBinding]) -> Option<String> {
    let trimmed = statement.trim().trim_end_matches(';');
    for binding in bindings.iter().filter(|binding| binding.reactive) {
        let prefix = format!("{} =", binding.name);
        if trimmed.starts_with(&prefix) {
            let rhs = trimmed[prefix.len()..].trim();
            let rhs = substitute_reactive_reads(rhs, bindings);
            let setter = match binding.value_type {
                SignalValueType::I32 => format!("sig_set_i32({}, {rhs});", binding.sig_var),
                SignalValueType::Utf8 => format!("sig_set_utf8({}, {rhs});", binding.sig_var),
                SignalValueType::Bool => format!("sig_set_bool({}, {rhs});", binding.sig_var),
            };
            return Some(setter);
        }
    }
    None
}

fn substitute_reactive_reads_in_line(line: &str, bindings: &[ScriptBinding]) -> String {
    let trimmed = line.trim();
    if trimmed.starts_with("micro ") || trimmed.starts_with("class ") || trimmed.starts_with("trait ") {
        return line.to_string();
    }
    substitute_reactive_reads(line, bindings)
}

fn substitute_reactive_reads(expr: &str, bindings: &[ScriptBinding]) -> String {
    let mut out = expr.to_string();
    for binding in bindings.iter().filter(|binding| binding.reactive) {
        out = replace_binding_ident(&out, &binding.name, &binding.read_expr());
    }
    out
}

fn replace_binding_ident(expr: &str, name: &str, replacement: &str) -> String {
    let mut out = String::new();
    let name_bytes = name.as_bytes();
    let bytes = expr.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index..].starts_with(name_bytes) {
            let before_ok = index == 0 || !is_ident_char(bytes[index - 1]);
            let after_index = index + name_bytes.len();
            let after_ok = after_index >= bytes.len() || !is_ident_char(bytes[after_index]);
            if before_ok && after_ok {
                out.push_str(replacement);
                index = after_index;
                continue;
            }
        }
        out.push(bytes[index] as char);
        index += 1;
    }
    out
}

fn is_ident_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// 提取 script 块中的顶层 `micro` / `class` / `trait` 声明（含完整函数体）。
fn extract_module_declarations(script: &str) -> String {
    let lines: Vec<&str> = script.lines().collect();
    let mut out = String::new();
    let mut index = 0usize;
    while index < lines.len() {
        let trimmed = lines[index].trim();
        if trimmed.starts_with("micro ") || trimmed.starts_with("class ") || trimmed.starts_with("trait ") {
            let (block, next) = take_braced_block(&lines, index);
            out.push_str(&block);
            if !block.ends_with('\n') {
                out.push('\n');
            }
            index = next;
            continue;
        }
        index += 1;
    }
    out
}

fn take_braced_block(lines: &[&str], start: usize) -> (String, usize) {
    let mut out = String::new();
    let mut depth = 0i32;
    let mut index = start;
    while index < lines.len() {
        let line = lines[index];
        out.push_str(line);
        out.push('\n');
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
            }
            else if ch == '}' {
                depth -= 1;
            }
        }
        index += 1;
        if depth == 0 && line.contains('{') {
            break;
        }
    }
    (out, index)
}

/// 从 script 块提取可放入 render 函数体的 `let` / `let mut` 绑定。
pub fn script_let_prelude(script: Option<&str>) -> String {
    let bindings = script.map(|source| abi_to_script_bindings(&extract_abi_for_script(source, "script").0)).unwrap_or_default();
    bindings.iter().map(ScriptBinding::as_v_let_line).collect::<Vec<_>>().join("\n    ")
}

fn lower_template_nodes(nodes: &[AwslTemplateNode]) -> SurfaceIr {
    nodes.iter().filter_map(lower_template_node).collect()
}

/// widget 下多个并列根节点时自动包一层 fragment（不增加 DOM 包裹标签）。
fn wrap_widget_roots_in_fragment(nodes: SurfaceIr) -> SurfaceIr {
    if nodes.len() <= 1 {
        return nodes;
    }
    let span = nodes.first().zip(nodes.last()).map(|(first, last)| span_union(first, last)).unwrap_or(0..0);
    vec![SurfaceNode::Fragment { children: nodes, span }]
}

fn span_union(first: &SurfaceNode, last: &SurfaceNode) -> std::ops::Range<usize> {
    let start = node_span(first).start;
    let end = node_span(last).end;
    start..end
}

fn node_span(node: &SurfaceNode) -> &std::ops::Range<usize> {
    match node {
        SurfaceNode::Tag { span, .. } | SurfaceNode::Text { span, .. } | SurfaceNode::Fragment { span, .. } => span,
        SurfaceNode::If(surface_if) => &surface_if.span,
        SurfaceNode::Loop(surface_loop) => &surface_loop.span,
    }
}

fn lower_template_node(node: &AwslTemplateNode) -> Option<SurfaceNode> {
    match node {
        AwslTemplateNode::Element(element) => Some(lower_element(element)),
        AwslTemplateNode::Text { content, span } => {
            if content.trim().is_empty() {
                return None;
            }
            Some(SurfaceNode::Text { parts: vec![SurfaceTextPart::Static(content.clone())], span: span.clone() })
        }
        AwslTemplateNode::Interpolation { expr, span } => {
            Some(SurfaceNode::Text { parts: vec![SurfaceTextPart::Dynamic(expr.clone())], span: span.clone() })
        }
    }
}

fn lower_element(element: &AwslElement) -> SurfaceNode {
    for directive in &element.directives {
        if let AwslDirectiveKind::If = directive.kind {
            let condition = directive.value.clone().unwrap_or_else(|| "true".into());
            let then_branch = lower_template_nodes(&element.children);
            return SurfaceNode::If(SurfaceIf { condition, then_branch, else_branch: Vec::new(), span: element.span.clone() });
        }
    }

    if element.tag == "loop" {
        if let Some((items_expr, item_var, key_expr)) = parse_loop_element_attrs(&element.attributes) {
            return SurfaceNode::Loop(SurfaceLoop {
                items_expr,
                item_var,
                index_var: "__idx".into(),
                key_expr,
                body: lower_template_nodes(&element.children),
                span: element.span.clone(),
            });
        }
    }

    if element.tag == "if" {
        let condition = element
            .attributes
            .iter()
            .find(|attr| attr.name == "condition" || attr.name == ":condition")
            .map(|attr| attr_value_expr(&attr.value))
            .unwrap_or_else(|| "true".into());
        let (then_nodes, else_nodes) = split_if_else_children(&element.children);
        return SurfaceNode::If(SurfaceIf {
            condition,
            then_branch: lower_template_nodes(&then_nodes),
            else_branch: lower_template_nodes(&else_nodes),
            span: element.span.clone(),
        });
    }

    if matches!(element.tag.as_str(), "fragment") {
        return SurfaceNode::Fragment { children: lower_template_nodes(&element.children), span: element.span.clone() };
    }

    let kind = classify_tag(&element.tag, element.is_component());
    let is_component_usage = matches!(kind, TemplateNodeKind::Component);
    let mut attrs = element
        .attributes
        .iter()
        .map(|attr| {
            let is_event = attr.name.starts_with("on:") || attr.name.starts_with('@');
            SurfaceAttr { name: attr.name.clone(), value: lower_attr_value(&attr.value), is_event, is_prop: is_component_usage && !is_event }
        })
        .collect::<Vec<SurfaceAttr>>();

    for directive in &element.directives {
        match &directive.kind {
            AwslDirectiveKind::On(event) => {
                let attr_name = format!("@{}", event);
                let value = directive.value.clone().unwrap_or_default();
                attrs.push(SurfaceAttr { name: attr_name, value: SurfaceAttrValue::Static(value), is_event: true, is_prop: false });
            }
            AwslDirectiveKind::Class => {
                if let Some(expr) = &directive.value {
                    merge_class_attr(&mut attrs, lower_class_directive_value(expr), is_component_usage);
                }
            }
            AwslDirectiveKind::Style => {
                if let Some(expr) = &directive.value {
                    let class_value = match lower_literal_attr_value(expr) {
                        SurfaceAttrValue::Static(text) if text.contains('?') => SurfaceAttrValue::Dynamic(expr.clone()),
                        other => other,
                    };
                    merge_class_attr(&mut attrs, class_value, is_component_usage);
                }
            }
            _ => {}
        }
    }

    let children = lower_template_nodes(&element.children);
    SurfaceNode::Tag { tag: element.tag.clone(), kind, attrs, children, span: element.span.clone() }
}

fn classify_tag(tag: &str, is_pascal_component: bool) -> TemplateNodeKind {
    // 标准 widget 原语优先于「PascalCase = 用户组件」启发式。
    if is_intrinsic_tag(tag) {
        return TemplateNodeKind::Intrinsic;
    }
    if is_pascal_component {
        return TemplateNodeKind::Component;
    }
    TemplateNodeKind::HostView
}

fn merge_class_attr(attrs: &mut Vec<SurfaceAttr>, value: SurfaceAttrValue, is_prop: bool) {
    if let Some(index) = attrs.iter().position(|attr| attr.name == "class") {
        let existing = attrs.remove(index);
        let merged = merge_attr_values(existing.value, value);
        attrs.push(SurfaceAttr { name: "class".into(), value: merged, is_event: false, is_prop });
        return;
    }
    attrs.push(SurfaceAttr { name: "class".into(), value, is_event: false, is_prop });
}

fn merge_attr_values(left: SurfaceAttrValue, right: SurfaceAttrValue) -> SurfaceAttrValue {
    match (left, right) {
        (SurfaceAttrValue::Static(a), SurfaceAttrValue::Static(b)) => {
            let merged = format!("{a} {b}").split_whitespace().collect::<Vec<_>>().join(" ");
            SurfaceAttrValue::Static(merged)
        }
        (SurfaceAttrValue::Static(a), SurfaceAttrValue::Dynamic(b)) | (SurfaceAttrValue::Dynamic(b), SurfaceAttrValue::Static(a)) => {
            SurfaceAttrValue::Mixed(vec![SurfaceTextPart::Static(format!("{a} ")), SurfaceTextPart::Dynamic(b)])
        }
        (SurfaceAttrValue::Static(a), SurfaceAttrValue::Mixed(mut parts)) => {
            parts.insert(0, SurfaceTextPart::Static(format!("{a} ")));
            SurfaceAttrValue::Mixed(parts)
        }
        (SurfaceAttrValue::Mixed(mut parts), SurfaceAttrValue::Static(b)) => {
            parts.push(SurfaceTextPart::Static(format!(" {b}")));
            SurfaceAttrValue::Mixed(parts)
        }
        (SurfaceAttrValue::Mixed(mut a), SurfaceAttrValue::Mixed(b)) => {
            a.extend(b);
            SurfaceAttrValue::Mixed(a)
        }
        (SurfaceAttrValue::Mixed(mut parts), SurfaceAttrValue::Dynamic(expr)) => {
            parts.push(SurfaceTextPart::Dynamic(expr));
            SurfaceAttrValue::Mixed(parts)
        }
        (SurfaceAttrValue::Dynamic(expr), SurfaceAttrValue::Mixed(mut parts)) => {
            parts.insert(0, SurfaceTextPart::Dynamic(expr));
            SurfaceAttrValue::Mixed(parts)
        }
        (SurfaceAttrValue::Dynamic(a), SurfaceAttrValue::Dynamic(b)) => {
            SurfaceAttrValue::Mixed(vec![SurfaceTextPart::Dynamic(a), SurfaceTextPart::Static(" ".into()), SurfaceTextPart::Dynamic(b)])
        }
    }
}

fn split_if_else_children(children: &[AwslTemplateNode]) -> (Vec<AwslTemplateNode>, Vec<AwslTemplateNode>) {
    for (index, child) in children.iter().enumerate() {
        if let AwslTemplateNode::Element(element) = child {
            if element.tag == "else" {
                let then_nodes = children[..index].to_vec();
                let else_nodes = if element.self_closing { children[index + 1..].to_vec() } else { element.children.clone() };
                return (then_nodes, else_nodes);
            }
        }
    }
    (children.to_vec(), Vec::new())
}

fn lower_attr_value(value: &AwslAttributeValue) -> SurfaceAttrValue {
    match value {
        AwslAttributeValue::Literal(text) => lower_literal_attr_value(text),
        AwslAttributeValue::Expression(expr) => SurfaceAttrValue::Dynamic(expr.clone()),
        AwslAttributeValue::Mixed(parts) => SurfaceAttrValue::Mixed(
            parts
                .iter()
                .map(|part| match part {
                    AwslTextPart::Text(text) => SurfaceTextPart::Static(text.clone()),
                    AwslTextPart::Expr(expr) => SurfaceTextPart::Dynamic(expr.clone()),
                })
                .collect(),
        ),
    }
}

fn lower_class_directive_value(expr: &str) -> SurfaceAttrValue {
    if expr.contains('{') {
        return lower_literal_attr_value(expr);
    }
    if is_plain_css_class_list(expr) {
        return SurfaceAttrValue::Static(expr.to_string());
    }
    SurfaceAttrValue::Dynamic(expr.to_string())
}

fn is_plain_css_class_list(expr: &str) -> bool {
    let trimmed = expr.trim();
    !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | ':'))
}

fn lower_literal_attr_value(text: &str) -> SurfaceAttrValue {
    // Support both `{{expr}}` (SSG style) and `{expr}` (AWSL attribute interpolations).
    if !text.contains('{') {
        return SurfaceAttrValue::Static(text.to_string());
    }
    let mut parts = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        if start > 0 {
            parts.push(SurfaceTextPart::Static(rest[..start].to_string()));
        }
        let double = rest[start + 1..].starts_with('{');
        let open_len = if double { 2 } else { 1 };
        let after = &rest[start + open_len..];
        let end = if double { after.find("}}") } else { find_single_brace_expr_end(after) };
        let Some(end) = end
        else {
            return SurfaceAttrValue::Static(text.to_string());
        };
        parts.push(SurfaceTextPart::Dynamic(after[..end].trim().to_string()));
        rest = &after[end + if double { 2 } else { 1 }..];
    }
    if !rest.is_empty() {
        parts.push(SurfaceTextPart::Static(rest.to_string()));
    }
    if parts.len() == 1 {
        return match &parts[0] {
            SurfaceTextPart::Static(s) => SurfaceAttrValue::Static(s.clone()),
            SurfaceTextPart::Dynamic(e) => SurfaceAttrValue::Dynamic(e.clone()),
        };
    }
    SurfaceAttrValue::Mixed(parts)
}

/// Index of the `}` that closes a `{expr}`, respecting strings and nested braces.
fn find_single_brace_expr_end(after: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = None::<char>;
    let mut index = 0usize;
    while index < after.len() {
        let ch = after[index..].chars().next()?;
        let ch_len = ch.len_utf8();
        if let Some(quote) = in_string {
            if ch == '\\' {
                index += ch_len + after[index + ch_len..].chars().next().map(|c| c.len_utf8()).unwrap_or(0);
                continue;
            }
            if ch == quote {
                in_string = None;
            }
            index += ch_len;
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_string = Some(ch);
            index += ch_len;
            continue;
        }
        match ch {
            '{' => depth += 1,
            '}' => {
                if depth == 0 {
                    return Some(index);
                }
                depth -= 1;
            }
            _ => {}
        }
        index += ch_len;
    }
    None
}

fn attr_value_expr(value: &AwslAttributeValue) -> String {
    match value {
        AwslAttributeValue::Literal(text) => format!("\"{text}\""),
        AwslAttributeValue::Expression(expr) => expr.clone(),
        AwslAttributeValue::Mixed(parts) => parts
            .iter()
            .map(|part| match part {
                AwslTextPart::Text(text) => text.clone(),
                AwslTextPart::Expr(expr) => expr.clone(),
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

fn parse_loop_element_attrs(attrs: &[oak_awsl::AwslAttribute]) -> Option<(String, String, Option<String>)> {
    let mut items_expr = None::<String>;
    let mut item_var = None::<String>;
    let mut key_expr = None::<String>;
    for attr in attrs {
        match attr.name.as_str() {
            "each" => items_expr = Some(attr_value_expr(&attr.value)),
            "item" => {
                if let AwslAttributeValue::Literal(name) = &attr.value {
                    item_var = Some(name.clone());
                }
            }
            "key" => key_expr = Some(attr_value_expr(&attr.value)),
            _ => {}
        }
    }
    Some((items_expr?, item_var?, key_expr))
}

fn abi_to_script_bindings(abi: &ComponentAbi) -> Vec<ScriptBinding> {
    let mut out = Vec::new();
    for property in &abi.properties {
        let init_expr = property.default_expr.clone().unwrap_or_else(|| default_expr_for_type(property.type_hint.as_deref()));
        out.push(ScriptBinding {
            name: property.name.clone(),
            init_expr: init_expr.clone(),
            kind: BindingKind::Property { required: property.required },
            reactive: false,
            sig_var: String::new(),
            value_type: infer_signal_type(property.type_hint.as_deref(), &init_expr),
        });
    }
    for state in &abi.states {
        let init_expr = state.init_expr.clone().unwrap_or_else(|| "false".into());
        out.push(ScriptBinding {
            name: state.name.clone(),
            init_expr,
            kind: BindingKind::ReactiveState,
            reactive: true,
            sig_var: String::new(),
            value_type: infer_signal_type(None, &state.init_expr.clone().unwrap_or_default()),
        });
    }
    for derived in &abi.derived {
        out.push(ScriptBinding {
            name: derived.name.clone(),
            init_expr: derived.expr.clone(),
            kind: BindingKind::Derived,
            reactive: false,
            sig_var: String::new(),
            value_type: infer_signal_type(None, &derived.expr),
        });
    }
    for memo in &abi.memoized {
        out.push(ScriptBinding {
            name: memo.name.clone(),
            init_expr: memo.expr.clone(),
            kind: BindingKind::Memoized,
            reactive: false,
            sig_var: String::new(),
            value_type: infer_signal_type(None, &memo.expr),
        });
    }
    out
}

fn default_expr_for_type(type_hint: Option<&str>) -> String {
    match type_hint {
        Some(hint) if hint.contains("bool") => "false".into(),
        Some(hint) if hint.contains("i32") || hint.contains("int") => "0".into(),
        Some(hint) if hint.contains("list") => "[]".into(),
        _ => "\"\"".into(),
    }
}

fn infer_signal_type(type_hint: Option<&str>, init_expr: &str) -> SignalValueType {
    if let Some(hint) = type_hint {
        if hint.contains("bool") {
            return SignalValueType::Bool;
        }
        if hint.contains("i32") || hint.contains("int") {
            return SignalValueType::I32;
        }
    }
    let trimmed = init_expr.trim();
    if trimmed == "true" || trimmed == "false" {
        return SignalValueType::Bool;
    }
    if trimmed.parse::<i32>().is_ok() {
        return SignalValueType::I32;
    }
    SignalValueType::Utf8
}

impl ScriptBinding {
    /// 转为 hydrate 函数体内的信号创建行。
    pub fn as_v_let_line(&self) -> String {
        if self.reactive {
            let creator = match self.value_type {
                SignalValueType::I32 => "sig_create_i32",
                SignalValueType::Utf8 => "sig_create_utf8",
                SignalValueType::Bool => "sig_create_bool",
            };
            format!("let {} = {}({})", self.sig_var, creator, self.init_expr)
        }
        else {
            format!("let {} = {}", self.name, self.init_expr)
        }
    }

    /// 读取响应式变量（生成 V 表达式）。
    pub fn read_expr(&self) -> String {
        if !self.reactive {
            return self.name.clone();
        }
        match self.value_type {
            SignalValueType::I32 => format!("sig_get_i32({})", self.sig_var),
            SignalValueType::Utf8 => format!("sig_get_utf8({})", self.sig_var),
            SignalValueType::Bool => format!("sig_get_bool({})", self.sig_var),
        }
    }

    /// 信号 id 变量名（用于 deps_csv）。
    pub fn sig_id_expr(&self) -> String {
        self.sig_var.clone()
    }
}
#[cfg(test)]
#[path = "../../tests/path_wired/awsl_lower.rs"]
mod awsl_lower_tests;
