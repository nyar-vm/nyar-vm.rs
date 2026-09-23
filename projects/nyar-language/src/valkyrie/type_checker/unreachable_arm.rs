//! 不可达 match arm 检测。
//!
//! 遍历模块中所有 `match` / `case` 表达式，逐个 arm 检查其 pattern 是否已被前序
//! 无条件 (unguarded) arm 完全覆盖。若完全覆盖则该 arm 永远不会被匹配到，标记为
//! 不可达并 emit 诊断。
//!
//! 仅 unguarded arm 会向后传递覆盖：带 guard 的 arm 即使 pattern 匹配，guard 也
//! 可能运行时失败而 fall through，因此不构成无条件覆盖。

use std::collections::{BTreeMap, BTreeSet};

use crate::types::{
    Identifier,
    hir::{
        HirBlock, HirExpr, HirExprKind, HirExtractorPattern, HirFunction, HirLiteral, HirMatchArm, HirModule, HirPattern, HirStatement,
        HirStatementKind, ValkyrieType,
    },
};

use super::last_name;

/// 不可达 match arm 检测错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreachableArmError {
    /// 错误类别。
    pub kind: UnreachableArmErrorKind,
    /// 人类可读错误信息。
    pub message: String,
}

/// 不可达 match arm 检测错误类别。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnreachableArmErrorKind {
    /// 该 arm 的 pattern 已被前序 arm 完全覆盖。
    UnreachableArm,
}

impl std::fmt::Display for UnreachableArmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for UnreachableArmError {}

/// 不可达 match arm 检测器。
///
/// 维护每个 match 表达式中已被前序 unguarded arm 覆盖的 pattern 集合，对每个
/// arm 判定其 pattern 是否完全落在已覆盖集合内。覆盖判定采用简化但实用的规则：
/// wildcard/variable 覆盖全部；literal/variant 命中即覆盖；Or pattern 需所有
/// 子 pattern 均被覆盖才判定为不可达；复合嵌套 pattern 保守不报以避免误报。
#[derive(Debug, Default)]
pub struct UnreachableArmChecker {
    errors: Vec<UnreachableArmError>,
}

impl UnreachableArmChecker {
    /// 创建新的检查器实例。
    pub fn new() -> Self {
        Self::default()
    }

    /// 返回已收集的错误列表。
    pub fn errors(&self) -> &[UnreachableArmError] {
        &self.errors
    }

    /// 检查整个模块中 match/case 表达式的 arm 不可达性。
    pub fn check_module(&mut self, module: &HirModule) -> Vec<UnreachableArmError> {
        self.errors.clear();
        for function in &module.functions {
            let mut env = type_env_from_params(function);
            self.walk_block(&function.body, &mut env);
        }
        for class in &module.structs {
            for method in &class.methods {
                let mut env = type_env_from_params(method);
                env.insert(Identifier::new("self"), ValkyrieType::Named(class.name.clone()));
                self.walk_block(&method.body, &mut env);
            }
        }
        for singleton in &module.singletons {
            for method in &singleton.methods {
                let mut env = type_env_from_params(method);
                env.insert(Identifier::new("self"), ValkyrieType::Named(singleton.name.clone()));
                self.walk_block(&method.body, &mut env);
            }
        }
        self.errors.clone()
    }

    fn walk_block(&mut self, block: &HirBlock, env: &mut BTreeMap<Identifier, ValkyrieType>) {
        for statement in &block.statements {
            self.walk_statement(statement, env);
        }
        if let Some(expr) = &block.expr {
            self.walk_expr(expr, env);
        }
    }

    fn walk_statement(&mut self, statement: &HirStatement, env: &mut BTreeMap<Identifier, ValkyrieType>) {
        match &statement.kind {
            HirStatementKind::Let { pattern, initializer, ty, .. } => {
                if let Some(value) = initializer {
                    self.walk_expr(value, env);
                }
                if let Some(ty) = ty {
                    if let HirPattern::Variable(name) = pattern {
                        env.insert(name.name.clone(), ty.clone());
                    }
                }
            }
            HirStatementKind::Expr(expr) => self.walk_expr(expr, env),
        }
    }

    fn walk_expr(&mut self, expr: &HirExpr, env: &mut BTreeMap<Identifier, ValkyrieType>) {
        match &expr.kind {
            HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
                self.walk_expr(scrutinee, env);
                for arm in arms {
                    self.walk_expr(&arm.body, env);
                    if let Some(guard) = &arm.guard {
                        self.walk_expr(guard, env);
                    }
                }
                self.check_match(arms);
            }
            HirExprKind::Call { callee, args, .. } => {
                self.walk_expr(callee, env);
                for arg in crate::types::hir::hir_call_arg_values(args) {
                    self.walk_expr(arg, env);
                }
            }
            HirExprKind::FieldAccess { object, .. } => self.walk_expr(object, env),
            HirExprKind::StoreField { object, value, .. } => {
                self.walk_expr(object, env);
                self.walk_expr(value, env);
            }
            HirExprKind::If { condition, then_branch, else_branch }
            | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
                self.walk_expr(condition, env);
                self.walk_block(then_branch, env);
                if let Some(else_branch) = else_branch {
                    self.walk_block(else_branch, env);
                }
            }
            HirExprKind::Block(block) => self.walk_block(block, env),
            HirExprKind::Lambda { body, params, .. } => {
                let mut nested = env.clone();
                for param in params {
                    nested.insert(param.name.name.clone(), param.ty.clone());
                }
                self.walk_block(body, &mut nested);
            }
            _ => {}
        }
    }

    fn check_match(&mut self, arms: &[HirMatchArm]) {
        let mut covered = CoveredSet::default();
        for arm in arms {
            if is_pattern_covered(&arm.pattern, &covered) {
                self.errors.push(UnreachableArmError {
                    kind: UnreachableArmErrorKind::UnreachableArm,
                    message: format!(
                        "unreachable match arm: this pattern is already covered by a previous arm ({})",
                        describe_pattern(&arm.pattern)
                    ),
                });
                // 已不可达的 arm 不再向后传递覆盖，避免对同一不可达 pattern 重复
                // 累积覆盖信息。
                continue;
            }
            // 仅 unguarded arm 向后传递无条件覆盖：带 guard 的 arm 即使 pattern
            // 匹配，guard 也可能运行时失败而 fall through。
            if arm.guard.is_none() {
                add_pattern_to_covered(&arm.pattern, &mut covered);
            }
        }
    }
}

fn describe_pattern(pattern: &HirPattern) -> String {
    match pattern {
        HirPattern::Wildcard => "_".to_string(),
        HirPattern::Else => "else".to_string(),
        HirPattern::Variable(name) => format!("var({})", name.name),
        HirPattern::Literal(lit) => format!("lit({lit:?})"),
        HirPattern::Name(name) | HirPattern::Type(name) => {
            format!("name({})", name.parts().iter().map(|p| p.as_str()).collect::<Vec<_>>().join("."))
        }
        HirPattern::Extractor(HirExtractorPattern::Constructor { name, fields, .. }) => {
            format!("extract({}/{})", name.parts().iter().map(|p| p.as_str()).collect::<Vec<_>>().join("."), fields.len())
        }
        HirPattern::Or(items) => format!("or({})", items.len()),
        HirPattern::Object { name: Some(name), .. } => {
            format!("object({})", name.parts().iter().map(|p| p.as_str()).collect::<Vec<_>>().join("."))
        }
        _ => "complex".to_string(),
    }
}

/// 已被前序 unguarded arm 覆盖的 pattern 集合。
#[derive(Debug, Default)]
struct CoveredSet {
    /// 是否已出现无条件 wildcard / variable / else arm（覆盖全部值域）。
    wildcard_seen: bool,
    /// 已被覆盖的字面量键集合。
    literals: BTreeSet<LiteralKey>,
    /// 已被覆盖的 sealed/enum 变体名集合。
    variants: BTreeSet<Identifier>,
}

/// 可比较的字面量键，仅处理整数与布尔；字符串/浮点等复杂字面量保守不纳入。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum LiteralKey {
    /// 整数字面量。
    Int(i64),
    /// 布尔字面量。
    Bool(bool),
    /// unit 字面量。
    Unit,
}

/// 将 `HirLiteral` 归一化为可比较键，无法识别时返回 None。
fn literal_key(lit: &HirLiteral) -> Option<LiteralKey> {
    match lit {
        HirLiteral::Integer64(n) => Some(LiteralKey::Int(*n)),
        HirLiteral::Bool(b) => Some(LiteralKey::Bool(*b)),
        HirLiteral::Unit => Some(LiteralKey::Unit),
        _ => None,
    }
}

/// 判定 pattern 是否完全落在已覆盖集合内。
///
/// 对 `Or` pattern 要求所有子 pattern 均被覆盖才视为不可达；对复合嵌套 pattern
/// (tuple/range/嵌套 extractor 等) 保守返回 false 以避免误报。
fn is_pattern_covered(pattern: &HirPattern, covered: &CoveredSet) -> bool {
    if covered.wildcard_seen {
        return true;
    }
    match pattern {
        HirPattern::Wildcard | HirPattern::Else | HirPattern::Variable(_) => {
            // 仅当已出现无条件 wildcard 时才视为被覆盖。
            covered.wildcard_seen
        }
        HirPattern::Literal(lit) => literal_key(lit).map(|key| covered.literals.contains(&key)).unwrap_or(false),
        HirPattern::Or(items) => {
            // 空 Or pattern 不报不可达；非空时所有子 pattern 均被覆盖才不可达。
            !items.is_empty() && items.iter().all(|item| is_pattern_covered(item, covered))
        }
        HirPattern::Extractor(HirExtractorPattern::Constructor { name, .. }) => {
            // 仅当 covered 中已有「整变体覆盖」时，同名构造器（任意载荷）才不可达。
            last_name(name).map(|variant| covered.variants.contains(&variant)).unwrap_or(false)
        }
        HirPattern::Object { name: Some(name), .. } => last_name(name).map(|variant| covered.variants.contains(&variant)).unwrap_or(false),
        HirPattern::Name(name) | HirPattern::Type(name) => last_name(name).map(|variant| covered.variants.contains(&variant)).unwrap_or(false),
        // 复合嵌套 pattern (tuple/range/array/bind/mut/pin/object-without-name 等)
        // 保守不报不可达。
        _ => false,
    }
}

/// 构造器载荷是否覆盖该变体的全部值域（字段均为不可辩驳绑定 / `_`）。
///
/// 含字面量等可辩驳子 pattern 时（如 `Punctuation("{")`）不视为整变体覆盖，
/// 否则同变体不同载荷的后续 arm 会被误报 unreachable。
fn constructor_covers_whole_variant(fields: &[HirPattern]) -> bool {
    fields.iter().all(|field| !field.refutability().is_refutable())
}

/// 将 unguarded arm 的 pattern 累积进已覆盖集合。
fn add_pattern_to_covered(pattern: &HirPattern, covered: &mut CoveredSet) {
    match pattern {
        HirPattern::Wildcard | HirPattern::Else | HirPattern::Variable(_) => {
            covered.wildcard_seen = true;
        }
        HirPattern::Literal(lit) => {
            if let Some(key) = literal_key(lit) {
                covered.literals.insert(key);
            }
        }
        HirPattern::Or(items) => {
            for item in items {
                add_pattern_to_covered(item, covered);
            }
        }
        HirPattern::Extractor(HirExtractorPattern::Constructor { name, fields, .. }) => {
            // 仅不可辩驳载荷才整变体覆盖；可辩驳载荷保守不写入，避免误伤分派臂。
            if constructor_covers_whole_variant(fields) {
                if let Some(variant) = last_name(name) {
                    covered.variants.insert(variant);
                }
            }
        }
        HirPattern::Object { name: Some(name), fields, .. } => {
            if fields.iter().all(|(_, p)| !p.refutability().is_refutable()) {
                if let Some(variant) = last_name(name) {
                    covered.variants.insert(variant);
                }
            }
        }
        HirPattern::Name(name) | HirPattern::Type(name) => {
            if let Some(variant) = last_name(name) {
                covered.variants.insert(variant);
            }
        }
        // 复合嵌套 pattern 不纳入覆盖集合，避免对后续 arm 产生误判。
        _ => {}
    }
}

fn type_env_from_params(function: &HirFunction) -> BTreeMap<Identifier, ValkyrieType> {
    function.params.iter().map(|param| (param.name.name.clone(), param.ty.clone())).collect()
}
