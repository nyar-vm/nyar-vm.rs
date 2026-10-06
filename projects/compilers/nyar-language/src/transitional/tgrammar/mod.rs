//! `tgrammar` 过渡解析器（自 `vcc-data` 迁入，仅供单元测试与迁移对照）。
//!
//! 生产 Compiler 不得调用本模块；`<% match arch %>` 必须由 Resolver 或 Oak 模板前端
//! 在源码进入 `frontend::parse_source` 之前展开。

mod ast;
mod lexer;
mod parser;

pub use ast::{TgIf, TgIfArm, TgKeyword, TgLoop, TgMatch, TgMatchArm, TgNode, TgRoot, TgTextPart};
pub use lexer::{Lexer, Token, TokenKind};
pub use parser::{TgParseError, parse_tgrammar_fragment, parse_tgrammar_template};

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
        match parse_tgrammar_fragment(&source[abs..]) {
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
