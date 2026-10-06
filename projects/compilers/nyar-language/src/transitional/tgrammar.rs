//! `tgrammar` 模板预处理（过渡 `vcc-data`）。
//!
//! 目标模板中的 `<% match arch %>` 片段在 Compiler 内展开；长期应迁入 Oak
//! `oak-valkyrie` 或独立模板前端，不在此扩展语法。

use vcc_data::text::valkyrie::tgrammar::{TgIf, TgLoop, TgMatch, TgNode};

/// 按目标架构展开源码中的 `tgrammar` 模板片段。
pub fn preprocess_target_templates(source: &str, arch: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut pos = 0;
    while pos < source.len() {
        let Some(rel) = source[pos..].find("<% match ")
        else {
            result.push_str(&source[pos..]);
            break;
        };
        let abs = pos + rel;
        result.push_str(&source[pos..abs]);
        match vcc_data::text::valkyrie::tgrammar::parse_tgrammar_fragment(&source[abs..]) {
            Ok((nodes, consumed)) if nodes.len() == 1 => {
                let fragment = &source[abs..abs + consumed];
                if let TgNode::Match(match_node) = &nodes[0]
                    && match_node.scrutinee.trim() == "arch"
                {
                    let selected = select_arch_match_body(match_node, arch);
                    result.push_str(&preprocess_target_templates(&tg_root_to_source(selected, fragment), arch));
                }
                else {
                    result.push_str(fragment);
                }
                pos = abs + consumed;
            }
            _ => {
                result.push_str(&source[abs..abs + 2]);
                pos = abs + 2;
            }
        }
    }
    result
}

fn select_arch_match_body<'a>(match_node: &'a TgMatch, arch: &str) -> &'a [TgNode] {
    for arm in &match_node.arms {
        if arm.pattern.as_deref().map(normalize_case_pattern).as_deref() == Some(arch) {
            return &arm.body;
        }
    }
    match_node.arms.iter().find(|arm| arm.pattern.is_none()).map(|arm| arm.body.as_slice()).unwrap_or(&[])
}

fn normalize_case_pattern(pattern: &str) -> String {
    let pattern = pattern.trim();
    if (pattern.starts_with('"') && pattern.ends_with('"')) || (pattern.starts_with('\'') && pattern.ends_with('\'')) {
        pattern[1..pattern.len().saturating_sub(1)].to_string()
    }
    else {
        pattern.to_string()
    }
}

fn tg_root_to_source(nodes: &[TgNode], fragment: &str) -> String {
    nodes.iter().map(|node| node_to_source(node, fragment)).collect()
}

fn node_to_source(node: &TgNode, fragment: &str) -> String {
    let span = match node {
        TgNode::Text { span, .. } | TgNode::Stmt { span, .. } | TgNode::Comment { span, .. } => span.clone(),
        TgNode::If(TgIf { span, .. }) | TgNode::Loop(TgLoop { span, .. }) | TgNode::Match(TgMatch { span, .. }) => span.clone(),
    };
    fragment[span].to_string()
}
