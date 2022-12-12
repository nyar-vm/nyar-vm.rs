//! 跨语言 `RewriteTheory` manifest 加载（与 C# `Nyar.Dialect.Theory` 对齐）。

use nyar_optimizer::{FutamuraProjectionFamily, RewriteEquation, RewritePhase, RewriteRule, RewriteTheory, TermRewrite, parse_pattern};
use nyar_types::{CapabilityTag, Identifier, QualifiedName};
use serde::Deserialize;

/// Manifest v1 根对象。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RewriteTheoryManifestV1 {
    /// 片段名。
    pub fragment: String,
    /// 规则列表。
    #[serde(default)]
    pub rules: Vec<RewriteTheoryRuleEntryV1>,
    /// 等式列表。
    #[serde(default)]
    pub equations: Vec<RewriteTheoryEquationEntryV1>,
    /// 显式结构化项级重写（优先于 rules/equations 中的 pattern 推断）。
    #[serde(default)]
    pub term_rewrites: Vec<RewriteTheoryTermRewriteEntryV1>,
}

/// 单条规则。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RewriteTheoryRuleEntryV1 {
    pub name: String,
    pub left: String,
    pub right: String,
    pub phase: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// 单条等式。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RewriteTheoryEquationEntryV1 {
    pub left: String,
    pub right: String,
    pub phase: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// 单条显式结构化重写。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RewriteTheoryTermRewriteEntryV1 {
    pub name: String,
    pub left: String,
    pub right: String,
    pub phase: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl RewriteTheoryManifestV1 {
    /// 从 JSON 文本解析 manifest。
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// 转为 `RewriteTheory`。
    pub fn to_rewrite_theory(&self) -> RewriteTheory {
        let mut theory = RewriteTheory::default();
        let projection = match self.fragment.as_str() {
            "graphic" | "neural" => vec![FutamuraProjectionFamily::Gpu],
            _ => vec![FutamuraProjectionFamily::Gpu],
        };
        for rule in &self.rules {
            theory.register(RewriteRule {
                name: Identifier::new(&rule.name),
                phase: parse_phase(&rule.phase),
                required_capabilities: rule.capabilities.iter().map(|cap| CapabilityTag::new(cap)).collect(),
                allowed_projection_families: projection.clone(),
            });
            register_manifest_entry(&mut theory, &rule.name, &rule.left, &rule.right, &rule.phase, &rule.capabilities);
        }
        for equation in &self.equations {
            register_manifest_entry(
                &mut theory,
                &format!("equation.{}", equation.left),
                &equation.left,
                &equation.right,
                &equation.phase,
                &equation.capabilities,
            );
        }
        for rewrite in &self.term_rewrites {
            if let Some(term_rewrite) =
                term_rewrite_from_strings(&rewrite.name, &rewrite.left, &rewrite.right, &rewrite.phase, &rewrite.capabilities)
            {
                theory.register_term_rewrite(term_rewrite);
            }
        }
        theory
    }
}

fn register_manifest_entry(theory: &mut RewriteTheory, name: &str, left: &str, right: &str, phase: &str, capabilities: &[String]) {
    if let Some(term_rewrite) = term_rewrite_from_strings(name, left, right, phase, capabilities) {
        theory.register_term_rewrite(term_rewrite);
        return;
    }
    theory.equate(RewriteEquation {
        left: qualified_from_dotted(left),
        right: qualified_from_dotted(right),
        phase: parse_phase(phase),
        required_capabilities: capabilities.iter().map(|cap| CapabilityTag::new(cap)).collect(),
    });
}

fn looks_like_term_pattern(value: &str) -> bool {
    value.contains('(') || value.contains('?')
}

fn term_rewrite_from_strings(name: &str, left: &str, right: &str, phase: &str, capabilities: &[String]) -> Option<TermRewrite> {
    if !looks_like_term_pattern(left) && !looks_like_term_pattern(right) {
        return None;
    }
    let left_pattern = parse_pattern(left).ok()?;
    let right_pattern = parse_pattern(right).ok()?;
    Some(
        TermRewrite::new(name, left_pattern, right_pattern, parse_phase(phase))
            .with_capabilities(capabilities.iter().map(|cap| CapabilityTag::new(cap)).collect()),
    )
}

fn qualified_from_dotted(value: &str) -> QualifiedName {
    QualifiedName::new(value.split('.').map(Identifier::new).collect())
}

fn parse_phase(value: &str) -> RewritePhase {
    match value {
        "local" | "normalize" => RewritePhase::Normalize,
        "algebraic" | "saturate" => RewritePhase::Saturate,
        "cleanup" | "fusion" | "pre-projection" => RewritePhase::PreProjection,
        _ => RewritePhase::Normalize,
    }
}

/// 内置 graphic manifest（与 C# 导出默认值对齐）。
pub fn builtin_graphic_manifest() -> RewriteTheoryManifestV1 {
    serde_json::from_str(include_str!("../../schemas/graphic.rewrite-theory.v1.json")).expect("graphic manifest")
}

/// 内置 neural manifest（与 C# 导出默认值对齐）。
pub fn builtin_neural_manifest() -> RewriteTheoryManifestV1 {
    serde_json::from_str(include_str!("../../schemas/neural.rewrite-theory.v1.json")).expect("neural manifest")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_pattern_rule_produces_term_rewrite() {
        let manifest = RewriteTheoryManifestV1::from_json(
            r#"{
                "fragment": "graphic",
                "rules": [
                    {
                        "name": "add-identity",
                        "left": "core.add(?x, 0)",
                        "right": "?x",
                        "phase": "local",
                        "capabilities": []
                    }
                ]
            }"#,
        )
        .expect("manifest");
        let theory = manifest.to_rewrite_theory();
        assert_eq!(theory.term_rewrites.len(), 1);
        assert!(theory.equations.is_empty());
        assert_eq!(theory.term_rewrites[0].name.as_str(), "add-identity");
    }

    #[test]
    fn manifest_flat_rule_still_produces_equation() {
        let theory = builtin_graphic_manifest().to_rewrite_theory();
        assert!(!theory.equations.is_empty());
    }
}
