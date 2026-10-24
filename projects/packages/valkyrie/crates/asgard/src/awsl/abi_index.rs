//! Project-wide component ABI index and cross-file validation (asgard render IR).

use oak_awsl::{
    AbiIssue, AbiIssueKind, AbiSeverity, ComponentAbi, awsl_stem_from_component_tag, extract_component_abi_from_script, is_snake_case,
    normalize_event_name, normalize_prop_name,
};

use super::{LoweredComponent, RenderAttr, RenderIr, RenderModule, RenderNode, render_ir::region_nodes};

pub use oak_awsl::ComponentAbiIndex;

/// Build an index from lowered components.
pub fn component_abi_index_from_components(components: &[LoweredComponent]) -> ComponentAbiIndex {
    ComponentAbiIndex::from_entries(components.iter().map(|component| (component.name.clone(), component.component_abi.clone())))
}

/// Validate template `:prop` / `@event` bindings against indexed component ABIs.
pub fn validate_component_abi(index: &ComponentAbiIndex, component: &LoweredComponent) -> Vec<AbiIssue> {
    let mut issues = component.abi_issues.clone();
    validate_render_ir(&component.render_ir, &mut issues, index);
    issues
}

fn validate_render_ir(module: &RenderIr, issues: &mut Vec<AbiIssue>, index: &ComponentAbiIndex) {
    for &root_id in &module.roots {
        validate_node(module, root_id, issues, index);
    }
}

fn validate_node(module: &RenderModule, node_id: super::render_ir::RenderNodeId, issues: &mut Vec<AbiIssue>, index: &ComponentAbiIndex) {
    let node = module.node(node_id).clone();
    match &node {
        RenderNode::Component(component) => {
            let target = awsl_stem_from_component_tag(&component.tag);
            let target_abi = index.get(&target);
            for attr in &component.attrs {
                if attr.is_prop {
                    let prop_name = normalize_prop_name(&attr.name);
                    if !is_snake_case(&prop_name) {
                        issues.push(AbiIssue {
                            kind: AbiIssueKind::NotSnakeCase,
                            message: format!("`:{prop_name}` must be snake_case"),
                            span: None,
                            severity: AbiSeverity::Warning,
                        });
                    }
                    if let Some(abi) = target_abi {
                        if abi.property(&prop_name).is_none() {
                            issues.push(AbiIssue {
                                kind: AbiIssueKind::PropertyOnNonLet,
                                message: format!("`:{prop_name}` is not declared on component `{target}`"),
                                span: None,
                                severity: AbiSeverity::Error,
                            });
                        }
                    }
                }
                if attr.is_event {
                    let event_name = normalize_event_name(&attr.name);
                    if !is_snake_case(&event_name) {
                        issues.push(AbiIssue {
                            kind: AbiIssueKind::NotSnakeCase,
                            message: format!("`@{event_name}` must be snake_case"),
                            span: None,
                            severity: AbiSeverity::Warning,
                        });
                    }
                    if let Some(abi) = target_abi {
                        if abi.event(&event_name).is_none() {
                            issues.push(AbiIssue {
                                kind: AbiIssueKind::EventOnNonMicro,
                                message: format!("`@{event_name}` is not declared on component `{target}`"),
                                span: None,
                                severity: AbiSeverity::Error,
                            });
                        }
                    }
                }
            }
            validate_region(module, component.children, issues, index);
        }
        RenderNode::Element(element) => validate_region(module, element.children, issues, index),
        RenderNode::If(render_if) => {
            validate_region(module, render_if.then_region, issues, index);
            validate_region(module, render_if.else_region, issues, index);
        }
        RenderNode::Loop(render_loop) => validate_region(module, render_loop.body_region, issues, index),
        RenderNode::Fragment(fragment) => validate_region(module, fragment.children, issues, index),
        RenderNode::Text { .. } => {}
    }
}

fn validate_region(module: &RenderModule, region: super::render_ir::RenderRegionId, issues: &mut Vec<AbiIssue>, index: &ComponentAbiIndex) {
    for &node_id in region_nodes(module, region) {
        validate_node(module, node_id, issues, index);
    }
}

/// Extract ABI + issues from `<script>` text.
pub fn extract_abi_for_script(script: &str, widget_name: &str) -> (ComponentAbi, Vec<AbiIssue>) {
    let result = extract_component_abi_from_script(script, widget_name);
    (result.abi, result.issues)
}

/// Refine template attribute flags using the ABI index when lowering.
pub fn refine_component_attr_flags(tag: &str, attr: &mut RenderAttr, index: Option<&ComponentAbiIndex>) {
    let Some(index) = index
    else {
        return;
    };
    let (is_prop, is_event) = oak_awsl::refine_binding_kind(tag, &attr.name, attr.is_event, index);
    attr.is_prop = is_prop;
    attr.is_event = is_event;
}
