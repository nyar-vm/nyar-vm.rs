use super::super::{
    canonical::{RenderAttr, RenderAttrValue, RenderModule, RenderNode},
    ids::{RenderNodeId, RenderRegionId},
    surface::TemplateNodeKind,
};

/// 主题注册表 pass 的配置：单一主题/模式可折叠掉 ThemeSwitcher。
#[derive(Debug, Clone, Copy, Default)]
pub struct ThemeRegistryOptions<'a> {
    /// 固定主题名；为 `None` 时该 pass 直接返回。
    pub single_theme: Option<&'a str>,
    /// 固定模式名；为 `None` 时该 pass 直接返回。
    pub single_mode: Option<&'a str>,
}

/// 折叠 ThemeShell/ThemeSwitcher 并把选定的主题与模式固化为 class。
pub fn theme_registry_pass(module: &mut RenderModule, options: ThemeRegistryOptions<'_>) {
    let ThemeRegistryOptions { single_theme, single_mode } = options;
    if single_theme.is_none() || single_mode.is_none() {
        return;
    }

    let roots = module.roots.clone();
    let mut kept_roots = Vec::new();
    for node_id in roots {
        transform_theme_node(module, node_id, single_theme, single_mode);
        let node = module.node(node_id).clone();
        if should_drop_theme_switcher(&node) {
            module.nodes[node_id.0 as usize] =
                RenderNode::Fragment(super::super::canonical::RenderFragment { children: RenderRegionId::EMPTY, span: 0..0 });
            continue;
        }
        kept_roots.push(node_id);
        recurse_theme_children(module, &node, single_theme, single_mode);
    }
    module.roots = kept_roots;

    for region_index in 0..module.regions.len() {
        apply_theme_registry_to_region(module, RenderRegionId(region_index as u32), single_theme, single_mode);
    }
}

fn apply_theme_registry_to_region(module: &mut RenderModule, region_id: RenderRegionId, single_theme: Option<&str>, single_mode: Option<&str>) {
    if !region_id.is_valid() {
        return;
    }
    let node_ids = module.region(region_id).nodes.clone();
    let mut kept = Vec::new();
    for node_id in node_ids {
        transform_theme_node(module, node_id, single_theme, single_mode);
        let node = module.node(node_id).clone();
        if should_drop_theme_switcher(&node) {
            module.nodes[node_id.0 as usize] =
                RenderNode::Fragment(super::super::canonical::RenderFragment { children: RenderRegionId::EMPTY, span: 0..0 });
            continue;
        }
        kept.push(node_id);
        recurse_theme_children(module, &node, single_theme, single_mode);
    }
    module.regions[region_id.0 as usize].nodes = kept;
}

fn should_drop_theme_switcher(node: &RenderNode) -> bool {
    match node {
        RenderNode::Element(element) => element.tag == "ThemeSwitcher",
        RenderNode::Component(component) => component.tag == "ThemeSwitcher",
        _ => false,
    }
}

fn transform_theme_node(module: &mut RenderModule, node_id: RenderNodeId, single_theme: Option<&str>, single_mode: Option<&str>) {
    let node = module.node(node_id).clone();
    match node {
        RenderNode::Element(mut element) if element.tag == "ThemeShell" => {
            if let Some(theme) = single_theme {
                set_tag_attr(&mut element.attrs, "theme", RenderAttrValue::Static(theme.to_string()), true);
            }
            if let Some(mode) = single_mode {
                set_tag_attr(&mut element.attrs, "mode", RenderAttrValue::Static(mode.to_string()), true);
            }
            if let (Some(theme), Some(mode)) = (single_theme, single_mode) {
                element.tag = "Box".into();
                element.kind = TemplateNodeKind::Intrinsic;
                element.attrs.retain(|attr| !matches!(attr.name.as_str(), "theme" | "mode" | "themes" | "modes"));
                set_tag_attr(&mut element.attrs, "class", RenderAttrValue::Static(format!("theme-shell theme-{theme}-{mode}")), false);
            }
            module.nodes[node_id.0 as usize] = RenderNode::Element(element);
        }
        RenderNode::Component(mut component) if component.tag == "ThemeShell" => {
            if let (Some(theme), Some(mode)) = (single_theme, single_mode) {
                component.tag = "Box".into();
                component.attrs.retain(|attr| !matches!(attr.name.as_str(), "theme" | "mode" | "themes" | "modes"));
                set_tag_attr(&mut component.attrs, "class", RenderAttrValue::Static(format!("theme-shell theme-{theme}-{mode}")), false);
            }
            module.nodes[node_id.0 as usize] = RenderNode::Component(component);
        }
        _ => {}
    }
}

fn recurse_theme_children(module: &mut RenderModule, node: &RenderNode, single_theme: Option<&str>, single_mode: Option<&str>) {
    match node {
        RenderNode::Element(element) => apply_theme_registry_to_region(module, element.children, single_theme, single_mode),
        RenderNode::Component(component) => apply_theme_registry_to_region(module, component.children, single_theme, single_mode),
        RenderNode::Fragment(fragment) => apply_theme_registry_to_region(module, fragment.children, single_theme, single_mode),
        RenderNode::If(render_if) => {
            apply_theme_registry_to_region(module, render_if.then_region, single_theme, single_mode);
            apply_theme_registry_to_region(module, render_if.else_region, single_theme, single_mode);
        }
        RenderNode::Loop(render_loop) => apply_theme_registry_to_region(module, render_loop.body_region, single_theme, single_mode),
        RenderNode::Text(_) => {}
    }
}

fn set_tag_attr(attrs: &mut Vec<RenderAttr>, name: &str, value: RenderAttrValue, is_prop: bool) {
    if let Some(attr) = attrs.iter_mut().find(|attr| attr.name == name) {
        attr.value = value;
        attr.is_event = false;
        attr.is_prop = is_prop;
        return;
    }
    attrs.push(RenderAttr { name: name.into(), value, is_event: false, is_prop, event_id: None });
}
