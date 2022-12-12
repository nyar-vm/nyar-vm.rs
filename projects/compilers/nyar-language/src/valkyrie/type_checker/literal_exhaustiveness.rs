//! Literal 与 range pattern-space 穷尽性检查。

use std::collections::{BTreeMap, BTreeSet};

use crate::types::{
    Identifier,
    hir::{
        HirBlock, HirExpr, HirExprKind, HirFunction, HirLiteral, HirMatchArm, HirModule, HirPattern, HirStatement, HirStatementKind,
        ValkyrieType,
    },
};

use super::last_name;

/// literal/range 维度穷尽性检查错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiteralExhaustivenessError {
    /// 错误类别。
    pub kind: LiteralExhaustivenessErrorKind,
    /// 人类可读错误信息。
    pub message: String,
}

/// literal/range 穷尽性检查错误类别。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiteralExhaustivenessErrorKind {
    /// 字面量/range match 未覆盖整个值域。
    NonExhaustiveLiteral,
    /// 出现重复的字面量 arm。
    DuplicateLiteral,
    /// 不同 range arm 之间存在重叠。
    OverlappingRange,
}

impl std::fmt::Display for LiteralExhaustivenessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for LiteralExhaustivenessError {}

/// literal 与 range pattern-space 穷尽性检查器。
///
/// 遍历模块中所有 `match` / `case` 表达式，当 scrutinee 为 bool / 整数 / char 类型
/// 且不存在无条件 wildcard 时，对 literal 与 range pattern 的覆盖值域进行穷尽性判定。
#[derive(Debug, Default)]
pub struct LiteralExhaustivenessChecker {
    errors: Vec<LiteralExhaustivenessError>,
}

impl LiteralExhaustivenessChecker {
    /// 创建新的检查器实例。
    pub fn new() -> Self {
        Self::default()
    }

    /// 返回已收集的错误列表。
    pub fn errors(&self) -> &[LiteralExhaustivenessError] {
        &self.errors
    }

    /// 检查整个模块中的 match/case 表达式的 literal/range 穷尽性。
    pub fn check_module(&mut self, module: &HirModule) -> Vec<LiteralExhaustivenessError> {
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
        // 无条件 wildcard / else / 变量绑定 / 全开 range (`..`) 视为完全覆盖。
        let has_wildcard = arms.iter().any(|arm| arm.guard.is_none() && is_wildcard_like_pattern(&arm.pattern));
        if has_wildcard {
            return;
        }

        let scrutinee_type = match resolve_scrutinee_type(scrutinee, env) {
            Some(ty) => ty,
            None => return,
        };
        let literal_space = match normalize_literal_type(&scrutinee_type) {
            Some(space) => space,
            None => return,
        };

        let (type_min, type_max) = match value_space_bounds(literal_space) {
            Some(bounds) => bounds,
            // LargeInteger: 精确边界不可用，使用占位值。
            None => (0, 0),
        };

        // 收集所有 arm 的 literal/range 覆盖项。Or pattern 递归扁平化。
        // 无条件覆盖 (unguarded) 用于穷尽性判定；所有 arm (含 guarded) 用于 duplicate/overlap。
        let mut unguarded_intervals: Vec<(i128, i128)> = Vec::new();
        let mut all_literals: Vec<i128> = Vec::new();
        let mut all_ranges: Vec<(i128, i128)> = Vec::new();

        for arm in arms {
            let items = flatten_literal_patterns(&arm.pattern, type_min, type_max);
            for item in items {
                match item {
                    LiteralItem::Point(val) => {
                        all_literals.push(val);
                        if arm.guard.is_none() {
                            unguarded_intervals.push((val, val));
                        }
                    }
                    LiteralItem::Range(start, end) => {
                        all_ranges.push((start, end));
                        if arm.guard.is_none() {
                            unguarded_intervals.push((start, end));
                        }
                    }
                }
            }
        }

        // 重复字面量检测：相同 literal 出现两次即报 duplicate。
        let mut seen_literals = BTreeSet::new();
        for val in &all_literals {
            if !seen_literals.insert(*val) {
                self.errors.push(LiteralExhaustivenessError {
                    kind: LiteralExhaustivenessErrorKind::DuplicateLiteral,
                    message: format!("duplicate literal match arm: {}", val),
                });
                break;
            }
        }

        // range 重叠检测：两个 range arm 区间相交即报 overlap。
        if self.errors.is_empty() {
            let mut sorted_ranges = all_ranges.clone();
            sorted_ranges.sort();
            'outer: for i in 0..sorted_ranges.len() {
                for j in (i + 1)..sorted_ranges.len() {
                    let (s1, e1) = sorted_ranges[i];
                    let (s2, e2) = sorted_ranges[j];
                    if s1 <= e2 && s2 <= e1 {
                        self.errors.push(LiteralExhaustivenessError {
                            kind: LiteralExhaustivenessErrorKind::OverlappingRange,
                            message: format!("overlapping range match arms: {}..={} and {}..={}", s1, e1, s2, e2),
                        });
                        break 'outer;
                    }
                }
            }
        }

        // 已报告 duplicate 或 overlap 时不再追加穷尽性错误，保证首个错误信息精确。
        if !self.errors.is_empty() {
            return;
        }

        // 穷尽性判定。
        let is_exhaustive = match literal_space {
            // 大值域整数 (i16 及以上) 无法精确判定，无 wildcard 时直接报 non-exhaustive。
            LiteralTypeSpace::LargeInteger => false,
            // bool / i8 / u8 / char 使用区间合并精确判定。
            _ => {
                let merged = merge_intervals(unguarded_intervals);
                covers_full_range(&merged, type_min, type_max)
            }
        };

        if !is_exhaustive {
            let type_name = describe_type(literal_space);
            let message = match literal_space {
                LiteralTypeSpace::LargeInteger => {
                    format!("non exhaustive literal match for {}: large integer type requires wildcard or else arm", type_name)
                }
                _ => {
                    format!("non exhaustive literal match for {}: literal/range arms do not cover the full value space", type_name)
                }
            };
            self.errors.push(LiteralExhaustivenessError { kind: LiteralExhaustivenessErrorKind::NonExhaustiveLiteral, message });
        }
    }
}

/// literal 维度的 scrutinee 类型分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiteralTypeSpace {
    /// bool: {false, true}
    Bool,
    /// i8: [-128, 127]
    I8,
    /// u8: [0, 255]
    U8,
    /// char: [0, 0x10FFFF]
    Char,
    /// 大值域整数 (i16 及以上)，无法精确判定，要求 wildcard 兜底。
    LargeInteger,
}

/// 从 ValkyrieType 推导 literal 维度类型分类，无法识别时返回 None。
fn normalize_literal_type(ty: &ValkyrieType) -> Option<LiteralTypeSpace> {
    match ty {
        ValkyrieType::Boolean => Some(LiteralTypeSpace::Bool),
        ValkyrieType::Integer8 { signed: true } => Some(LiteralTypeSpace::I8),
        ValkyrieType::Integer8 { signed: false } => Some(LiteralTypeSpace::U8),
        ValkyrieType::Character => Some(LiteralTypeSpace::Char),
        ValkyrieType::Integer16 { .. } | ValkyrieType::Integer32 { .. } | ValkyrieType::Integer64 { .. } | ValkyrieType::Integer128 { .. } => {
            Some(LiteralTypeSpace::LargeInteger)
        }
        // 类型推断阶段可能保留 Named 形式，此处一并归一化。
        ValkyrieType::Named(name) => match name.as_str() {
            "bool" => Some(LiteralTypeSpace::Bool),
            "i8" => Some(LiteralTypeSpace::I8),
            "u8" => Some(LiteralTypeSpace::U8),
            "char" => Some(LiteralTypeSpace::Char),
            "i16" | "u16" | "i32" | "u32" | "i64" | "u64" | "i128" | "u128" => Some(LiteralTypeSpace::LargeInteger),
            _ => None,
        },
        _ => None,
    }
}

/// 返回小值域类型的 [MIN, MAX] 边界；LargeInteger 返回 None。
fn value_space_bounds(space: LiteralTypeSpace) -> Option<(i128, i128)> {
    match space {
        LiteralTypeSpace::Bool => Some((0, 1)),
        LiteralTypeSpace::I8 => Some((-128, 127)),
        LiteralTypeSpace::U8 => Some((0, 255)),
        LiteralTypeSpace::Char => Some((0, 0x10FFFF)),
        LiteralTypeSpace::LargeInteger => None,
    }
}

/// 返回类型分类的可读名称，用于错误信息。
fn describe_type(space: LiteralTypeSpace) -> &'static str {
    match space {
        LiteralTypeSpace::Bool => "bool",
        LiteralTypeSpace::I8 => "i8",
        LiteralTypeSpace::U8 => "u8",
        LiteralTypeSpace::Char => "char",
        LiteralTypeSpace::LargeInteger => "integer",
    }
}

/// 判断 pattern 是否为 wildcard 等价（无条件时视为完全覆盖）。
fn is_wildcard_like_pattern(pattern: &HirPattern) -> bool {
    match pattern {
        HirPattern::Wildcard | HirPattern::Else | HirPattern::Variable(_) => true,
        // 全开 range `..` (start=None, end=None) 等价于 wildcard。
        HirPattern::Range { start: None, end: None, .. } => true,
        _ => false,
    }
}

/// 从 scrutinee 表达式解析其 ValkyrieType，依赖 let 绑定建立的环境。
fn resolve_scrutinee_type(scrutinee: &HirExpr, env: &BTreeMap<Identifier, ValkyrieType>) -> Option<ValkyrieType> {
    let name = match &scrutinee.kind {
        HirExprKind::Variable(identifier) => Some(identifier.name.clone()),
        HirExprKind::Path(path) => last_name(path),
        _ => None,
    }?;
    env.get(&name).cloned()
}

/// 扁平化后的 literal/range 覆盖项。
#[derive(Debug, Clone, Copy)]
enum LiteralItem {
    /// 单个字面量值。
    Point(i128),
    /// 闭区间 [start, end]。
    Range(i128, i128),
}

/// 将 HirLiteral 转换为 i128，仅处理整数与布尔字面量。
fn literal_to_i128(lit: &HirLiteral) -> Option<i128> {
    match lit {
        HirLiteral::Integer64(n) => Some(*n as i128),
        HirLiteral::Bool(b) => Some(if *b { 1 } else { 0 }),
        _ => None,
    }
}

/// 递归扁平化 pattern 中的 literal/range 覆盖项，Or pattern 递归展开。
/// type_min / type_max 用于补全 open-ended range 的缺失边界。
fn flatten_literal_patterns(pattern: &HirPattern, type_min: i128, type_max: i128) -> Vec<LiteralItem> {
    match pattern {
        HirPattern::Literal(lit) => {
            if let Some(val) = literal_to_i128(lit) {
                vec![LiteralItem::Point(val)]
            }
            else {
                Vec::new()
            }
        }
        HirPattern::Range { start, end, inclusive_end } => {
            let start_val = match start {
                Some(lit) => match literal_to_i128(lit) {
                    Some(v) => v,
                    None => return Vec::new(),
                },
                None => type_min,
            };
            let mut end_val = match end {
                Some(lit) => match literal_to_i128(lit) {
                    Some(v) => v,
                    None => return Vec::new(),
                },
                None => type_max,
            };
            // 排除上界 (`..` 而非 `..=`) 时区间右端收缩 1。
            if !inclusive_end {
                end_val = match end_val.checked_sub(1) {
                    Some(v) => v,
                    None => return Vec::new(),
                };
            }
            if end_val < start_val {
                return Vec::new();
            }
            vec![LiteralItem::Range(start_val, end_val)]
        }
        HirPattern::Or(patterns) => patterns.iter().flat_map(|p| flatten_literal_patterns(p, type_min, type_max)).collect(),
        _ => Vec::new(),
    }
}

/// 合并重叠或相邻的区间，返回有序不相交区间列表。
fn merge_intervals(mut intervals: Vec<(i128, i128)>) -> Vec<(i128, i128)> {
    if intervals.is_empty() {
        return Vec::new();
    }
    intervals.sort();
    let mut merged = vec![intervals[0]];
    for &(start, end) in &intervals[1..] {
        let last = merged.last_mut().unwrap();
        // 相邻 (start == last.1 + 1) 或重叠 (start <= last.1) 均合并。
        if start <= last.1.saturating_add(1) {
            last.1 = last.1.max(end);
        }
        else {
            merged.push((start, end));
        }
    }
    merged
}

/// 判断合并后的区间是否完整覆盖 [min, max]。
fn covers_full_range(merged: &[(i128, i128)], min: i128, max: i128) -> bool {
    if merged.is_empty() {
        return false;
    }
    if merged[0].0 > min {
        return false;
    }
    let mut covered_up_to = merged[0].1;
    for interval in &merged[1..] {
        if interval.0 > covered_up_to.saturating_add(1) {
            return false;
        }
        covered_up_to = covered_up_to.max(interval.1);
    }
    covered_up_to >= max
}

fn type_env_from_params(function: &HirFunction) -> BTreeMap<Identifier, ValkyrieType> {
    function.params.iter().map(|param| (param.name.name.clone(), param.ty.clone())).collect()
}
