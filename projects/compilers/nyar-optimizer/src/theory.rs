//! 内置 rewrite theory 与结构化重写构造。

use crate::{
    RewritePhase, RewriteTheory,
    egraph::StructuredRewrite,
    pattern::{TermPattern, atom_pattern, parse_pattern},
};
use nyar_types::{CapabilityTag, Identifier, QualifiedName};

/// 结构化项级重写。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermRewrite {
    /// 规则名。
    pub name: Identifier,
    /// 左侧模式。
    pub left: TermPattern,
    /// 右侧模式。
    pub right: TermPattern,
    /// 所属阶段。
    pub phase: RewritePhase,
    /// 触发规则所需能力。
    pub required_capabilities: Vec<CapabilityTag>,
}

impl TermRewrite {
    /// 构造一条结构化重写。
    pub fn new(name: impl Into<String>, left: TermPattern, right: TermPattern, phase: RewritePhase) -> Self {
        Self { name: Identifier::new(&name.into()), left, right, phase, required_capabilities: Vec::new() }
    }

    /// 附加所需能力。
    pub fn with_capabilities(mut self, capabilities: Vec<CapabilityTag>) -> Self {
        self.required_capabilities = capabilities;
        self
    }
}

/// 返回内置 `core` 常量折叠与恒等重写理论。
pub fn builtin_core_rewrite_theory() -> RewriteTheory {
    let mut theory = RewriteTheory::default();
    theory.term_rewrites.extend(builtin_core_term_rewrites());
    theory
}

/// 返回基础 `core` 结构化重写集合。
pub fn builtin_core_term_rewrites() -> Vec<TermRewrite> {
    vec![
        TermRewrite::new(
            "core.add-identity-left",
            parse_pattern("core.add(?x, 0)").expect("pattern"),
            parse_pattern("?x").expect("pattern"),
            RewritePhase::Normalize,
        ),
        TermRewrite::new(
            "core.add-identity-right",
            parse_pattern("core.add(0, ?x)").expect("pattern"),
            parse_pattern("?x").expect("pattern"),
            RewritePhase::Normalize,
        ),
        TermRewrite::new(
            "core.mul-zero-left",
            parse_pattern("core.mul(0, ?x)").expect("pattern"),
            parse_pattern("0").expect("pattern"),
            RewritePhase::Normalize,
        ),
        TermRewrite::new(
            "core.mul-zero-right",
            parse_pattern("core.mul(?x, 0)").expect("pattern"),
            parse_pattern("0").expect("pattern"),
            RewritePhase::Normalize,
        ),
        TermRewrite::new(
            "core.mul-one-left",
            parse_pattern("core.mul(1, ?x)").expect("pattern"),
            parse_pattern("?x").expect("pattern"),
            RewritePhase::Normalize,
        ),
        TermRewrite::new(
            "core.mul-one-right",
            parse_pattern("core.mul(?x, 1)").expect("pattern"),
            parse_pattern("?x").expect("pattern"),
            RewritePhase::Normalize,
        ),
        TermRewrite::new(
            "core.sub-self",
            parse_pattern("core.sub(?x, ?x)").expect("pattern"),
            parse_pattern("0").expect("pattern"),
            RewritePhase::Normalize,
        ),
    ]
}

/// 将 flat 等式转换为原子级结构化重写。
pub fn structured_rewrite_from_equation(name: impl Into<String>, left: QualifiedName, right: QualifiedName) -> StructuredRewrite {
    StructuredRewrite { name: Identifier::new(&name.into()), left: atom_pattern(left), right: atom_pattern(right) }
}

/// 将 [`TermRewrite`] 转为 [`StructuredRewrite`]。
pub fn structured_rewrite_from_term_rewrite(rewrite: &TermRewrite) -> StructuredRewrite {
    StructuredRewrite { name: rewrite.name.clone(), left: rewrite.left.clone(), right: rewrite.right.clone() }
}

/// 按阶段过滤结构化重写。
pub fn filter_term_rewrites<'a>(
    rewrites: impl IntoIterator<Item = &'a TermRewrite>,
    phase: RewritePhase,
    capabilities: &[CapabilityTag],
) -> Vec<StructuredRewrite> {
    rewrites
        .into_iter()
        .filter(|rewrite| rewrite.phase == phase)
        .filter(|rewrite| rewrite.required_capabilities.iter().all(|capability| capabilities.iter().any(|provided| provided == capability)))
        .map(structured_rewrite_from_term_rewrite)
        .collect()
}

/// 按阶段过滤 flat 等式。
pub fn filter_equations<'a>(
    equations: impl IntoIterator<Item = &'a crate::RewriteEquation>,
    phase: RewritePhase,
    capabilities: &[CapabilityTag],
) -> Vec<(QualifiedName, QualifiedName)> {
    equations
        .into_iter()
        .filter(|equation| equation.phase == phase)
        .filter(|equation| equation.required_capabilities.iter().all(|capability| capabilities.iter().any(|provided| provided == capability)))
        .map(|equation| (equation.left.clone(), equation.right.clone()))
        .collect()
}
