//! 复合 pattern（extractor / object / tuple）穷尽性检查。
//!
//! 现有的 `SealedMatchChecker` 覆盖 sealed/enum 维度，`LiteralExhaustivenessChecker`
//! 覆盖 literal/range 维度。但当 scrutinee 既不是 sealed/enum 也不是 literal 类型，
//! 且所有 arm 都是 refutable pattern（extractor、无 rest binding 的 object、含 literal
//! 的 tuple 等）时，此前完全不做穷尽性检查——只要没有无条件 wildcard 就直接跳过。
//!
//! 本检查器补齐这一空缺：遍历模块中所有 `match` / `case` 表达式，当 scrutinee 类型
//! 既非 sealed/enum 也非 literal 时，判定是否存在无条件覆盖（unguarded irrefutable
//! arm）。若所有 arm 都是 refutable 或带 guard，且没有无条件 wildcard/else/irrefutable
//! arm，则报 non-exhaustive，提示需要添加 `else` 兜底 arm。

use std::collections::{BTreeMap, BTreeSet};

use crate::types::{
    Identifier,
    hir::{HirBlock, HirExpr, HirExprKind, HirFunction, HirMatchArm, HirModule, HirPattern, HirStatement, HirStatementKind, ValkyrieType},
};

use super::{EnumRegistry, SealedClassRegistry, last_name, oop_checks::fill_sealed_class_registry};

/// 复合 pattern 穷尽性检查错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositePatternExhaustivenessError {
    /// 错误类别。
    pub kind: CompositePatternExhaustivenessErrorKind,
    /// 人类可读错误信息。
    pub message: String,
}

/// 复合 pattern 穷尽性检查错误类别。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositePatternExhaustivenessErrorKind {
    /// 所有 arm 都是 refutable（extractor/无 rest 的 object 等）或带 guard，且没有无条件 wildcard/else 兜底 arm。
    NonExhaustiveRefutableArms,
    /// scrutinee 为 tuple 类型，但没有任何 arm 的元数与之匹配，也没有 wildcard 兜底。
    NonExhaustiveTupleArity,
}

impl std::fmt::Display for CompositePatternExhaustivenessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CompositePatternExhaustivenessError {}

/// 复合 pattern（extractor / object / tuple）穷尽性检查器。
///
/// 遍历模块中所有 `match` / `case` 表达式。当 scrutinee 类型既非 sealed/enum 也非
/// literal 类型时，若不存在无条件覆盖（unguarded irrefutable 或 wildcard/else arm），
/// 则报 non-exhaustive。对 tuple 类型 scrutinee 额外检查元数覆盖。
#[derive(Debug, Default)]
pub struct CompositePatternExhaustivenessChecker {
    /// 已收集的错误列表。
    errors: Vec<CompositePatternExhaustivenessError>,
    /// sealed class 注册表，用于跳过 sealed 维度（由 `SealedMatchChecker` 负责）。
    sealed_registry: SealedClassRegistry,
    /// enum/sum type 注册表，用于跳过 enum 维度（由 `SealedMatchChecker` 负责）。
    enum_registry: EnumRegistry,
    /// 本模块定义的所有类型名称，用于区分本地类型与导入类型。
    /// 导入的 Named 类型可能是未知的 sum type / unite，保守跳过以避免误报。
    local_types: BTreeSet<Identifier>,
}

impl CompositePatternExhaustivenessChecker {
    /// 创建新的检查器实例。
    pub fn new() -> Self {
        Self::default()
    }

    /// 返回已收集的错误列表。
    pub fn errors(&self) -> &[CompositePatternExhaustivenessError] {
        &self.errors
    }

    /// 检查整个模块中 match/case 表达式的复合 pattern 穷尽性。
    pub fn check_module(&mut self, module: &HirModule) -> Vec<CompositePatternExhaustivenessError> {
        self.errors.clear();
        self.sealed_registry = fill_sealed_class_registry(module);
        self.enum_registry = EnumRegistry::from_module(module);
        self.local_types = collect_local_type_names(module);
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
                self.check_match(scrutinee, arms, env);
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

    fn check_match(&mut self, scrutinee: &HirExpr, arms: &[HirMatchArm], env: &BTreeMap<Identifier, ValkyrieType>) {
        let scrutinee_type = resolve_scrutinee_type(scrutinee, env);

        // 当 scrutinee 类型无法解析时（如 imply 块中的 self 未被注册到 env），
        // 无法判定穷尽性，保守跳过以避免误报。
        if scrutinee_type.is_none() {
            return;
        }

        // sealed/enum scrutinee 由 `SealedMatchChecker` 负责穷尽性判定，此处跳过以免重复报错。
        // `Result<T, E>` / `Option<T>` 等是 `Apply(Named(base), …)`，必须按 base 识别，
        // 否则 Fine/Fail·Some/None 会被误判成「全 refutable」而强迫加 `else`。
        // 对于未在本模块定义的 Named 类型（可能是其他模块导入的 sum type / unite），
        // 由于 `EnumRegistry` 仅包含本模块定义，无法判定其是否为 sum type，
        // 保守跳过以避免误报 non-exhaustive。
        // 本模块定义的普通 class（非 sealed、非 enum）不跳过，继续检查复合 pattern 穷尽性。
        if let Some(name) = nominal_type_name(scrutinee_type.as_ref()) {
            if self.sealed_registry.is_sealed_class(&name) || self.enum_registry.is_sum_type(&name) {
                return;
            }
            if !self.local_types.contains(&name) {
                return;
            }
        }

        // literal scrutinee（bool/整数/char）由 `LiteralExhaustivenessChecker` 负责，此处跳过。
        if let Some(ty) = &scrutinee_type {
            if is_literal_type(ty) {
                return;
            }
        }

        // 若存在无条件（unguarded）irrefutable arm，则该 arm 匹配全部取值，match 穷尽。
        // wildcard/else/variable 是 irrefutable 的特例，被一并覆盖。
        let has_unguarded_irrefutable = arms.iter().any(|arm| arm.guard.is_none() && is_irrefutable_pattern(&arm.pattern));
        if has_unguarded_irrefutable {
            return;
        }

        // tuple 元数穷尽性：当 scrutinee 类型为已知元数的 tuple 时，检查是否存在对应元数
        // 的 tuple arm 或 wildcard。若所有 tuple arm 元数均不匹配且无 wildcard，则报元数不匹配。
        if let Some(ValkyrieType::Tuple(field_types)) = &scrutinee_type {
            let arity = field_types.len();
            let has_matching_arity =
                arms.iter().any(|arm| has_matching_tuple_arity(&arm.pattern, arity) || is_wildcard_like_pattern(&arm.pattern));
            if !has_matching_arity {
                self.errors.push(CompositePatternExhaustivenessError {
                    kind: CompositePatternExhaustivenessErrorKind::NonExhaustiveTupleArity,
                    message: format!(
                        "non-exhaustive match: no arm with matching tuple arity {} for {}-tuple scrutinee; add a wildcard or `else` arm",
                        arity, arity
                    ),
                });
                return;
            }
        }

        // 无条件覆盖缺失：所有 arm 都是 refutable（extractor/无 rest 的 object/含 literal 的
        // tuple 等）或带 guard，且没有无条件 wildcard/else/irrefutable arm。需要兜底 `else` arm。
        //
        // sealed/enum scrutinee 已在上方提前 return，literal scrutinee 同理；故到达此处时
        // scrutinee 必为 tuple/class 等复合类型，refutable arm 无法保证穷尽，直接报错。
        self.errors.push(CompositePatternExhaustivenessError {
            kind: CompositePatternExhaustivenessErrorKind::NonExhaustiveRefutableArms,
            message: "non-exhaustive match: all arms are refutable or guarded; add an `else` fallback arm".to_string(),
        });
    }
}

/// 判断 pattern 是否为 wildcard 等价（无条件时视为完全覆盖）。
fn is_wildcard_like_pattern(pattern: &HirPattern) -> bool {
    matches!(pattern, HirPattern::Wildcard | HirPattern::Else | HirPattern::Variable(_))
}

/// 判断 pattern 是否为 irrefutable（不可失败），依据复合 pattern 穷尽性规则。
///
/// - `Wildcard` / `Variable` / `Else` → irrefutable
/// - `Tuple(items)` → 所有 items 均 irrefutable 则 irrefutable
/// - `Object { rest: Some(_), .. }` → rest binding 捕获剩余字段，irrefutable
/// - `Object { rest: None, .. }` → 无 rest binding，refutable
/// - `Or(items)` → 所有 items 均 irrefutable 则 irrefutable
/// - `Bind` / `Mut` / `Pin` → 内部 pattern irrefutable 则 irrefutable
/// - 其他（`Literal` / `Range` / `Extractor` / `Name` / `Type` / `TypedBind`）→ refutable
fn is_irrefutable_pattern(pattern: &HirPattern) -> bool {
    match pattern {
        HirPattern::Wildcard | HirPattern::Variable(_) | HirPattern::Else => true,
        HirPattern::Tuple(items) => items.iter().all(is_irrefutable_pattern),
        HirPattern::Object { rest: Some(_), .. } => true,
        HirPattern::Object { rest: None, .. } => false,
        HirPattern::Or(items) => items.iter().all(is_irrefutable_pattern),
        HirPattern::Bind { pattern, .. } => is_irrefutable_pattern(pattern),
        HirPattern::Mut(inner) => is_irrefutable_pattern(inner),
        HirPattern::Pin { pattern, .. } => is_irrefutable_pattern(pattern),
        _ => false,
    }
}

/// 判断 pattern（或其 Or 子 pattern）中是否存在元数匹配的 tuple pattern。
fn has_matching_tuple_arity(pattern: &HirPattern, arity: usize) -> bool {
    match pattern {
        HirPattern::Tuple(items) => items.len() == arity,
        HirPattern::Or(items) => items.iter().any(|item| has_matching_tuple_arity(item, arity)),
        _ => false,
    }
}

/// 判断类型是否为 literal 维度类型（bool/整数/char），由 `LiteralExhaustivenessChecker` 负责。
fn is_literal_type(ty: &ValkyrieType) -> bool {
    match ty {
        ValkyrieType::Boolean
        | ValkyrieType::Integer8 { .. }
        | ValkyrieType::Integer16 { .. }
        | ValkyrieType::Integer32 { .. }
        | ValkyrieType::Integer64 { .. }
        | ValkyrieType::Integer128 { .. }
        | ValkyrieType::Character => true,
        ValkyrieType::Named(name) => {
            matches!(name.as_str(), "bool" | "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64" | "i128" | "u128" | "char")
        }
        _ => false,
    }
}

/// 从 scrutinee 表达式解析其 `ValkyrieType`，依赖 let 绑定建立的环境。
fn resolve_scrutinee_type(scrutinee: &HirExpr, env: &BTreeMap<Identifier, ValkyrieType>) -> Option<ValkyrieType> {
    let name = match &scrutinee.kind {
        HirExprKind::Variable(identifier) => Some(identifier.name.clone()),
        HirExprKind::Path(path) => last_name(path),
        _ => None,
    }?;
    env.get(&name).cloned()
}

/// `Named(T)` 或 `Apply(Named(T), …)` → `T`（泛型 unite/enum 的 nominal 名）。
fn nominal_type_name(ty: Option<&ValkyrieType>) -> Option<Identifier> {
    match ty? {
        ValkyrieType::Named(name) => Some(name.clone()),
        ValkyrieType::Apply(base, _) => match base.as_ref() {
            ValkyrieType::Named(name) => Some(name.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn type_env_from_params(function: &HirFunction) -> BTreeMap<Identifier, ValkyrieType> {
    function.params.iter().map(|param| (param.name.name.clone(), param.ty.clone())).collect()
}

/// 收集本模块中定义的所有类型名称（class / enum / singleton 等），
/// 用于区分本地类型与导入类型。导入类型可能是未知的 sum type / unite，
/// 需保守跳过以避免误报 non-exhaustive。
fn collect_local_type_names(module: &HirModule) -> BTreeSet<Identifier> {
    let mut names = BTreeSet::new();
    for class in &module.structs {
        names.insert(class.name.clone());
    }
    for enum_def in &module.enums {
        names.insert(enum_def.name.clone());
    }
    for singleton in &module.singletons {
        names.insert(singleton.name.clone());
    }
    for flag in &module.flags {
        names.insert(flag.name.clone());
    }
    names
}
