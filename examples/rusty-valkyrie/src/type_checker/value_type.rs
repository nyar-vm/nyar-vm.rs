//! value 类型语义检查与字段写入扫描。

use super::path_helpers::last_name;
use crate::types::{
    Identifier, SourceSpan,
    hir::{HirBlock, HirExpr, HirExprKind, HirFunction, HirModule, HirPattern, HirStatement, HirStatementKind, ValkyrieType as HirType},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

/// 值类型语义违规的类别。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueTypeErrorKind {
    /// 值类型继承了非值类型。
    ValueTypeInheritance {
        /// 涉事值类型类名。
        class_name: Identifier,
        /// 被继承的父类名。
        parent: Identifier,
    },
    /// 对值类型字段执行了原地修改。
    ValueTypeFieldMutation {
        /// 字段所属值类型名。
        class_name: Identifier,
        /// 被修改的字段名。
        field: Identifier,
    },
}

/// 单条值类型违规：类别、渲染后的消息与源码位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueTypeError {
    /// 违规类别。
    pub kind: ValueTypeErrorKind,
    /// 人类可读的错误信息。
    pub message: String,
    /// 产生该违规的源码位置；不可定位时为 None。
    pub span: Option<SourceSpan>,
}

impl ValueTypeError {
    /// 构造「值类型继承非值类型」错误。
    pub fn value_type_inheritance(class_name: Identifier, parent: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: ValueTypeErrorKind::ValueTypeInheritance { class_name: class_name.clone(), parent: parent.clone() },
            message: format!("值类型 {} 不能继承 {}", class_name, parent),
            span,
        }
    }

    /// 构造「值类型字段被原地修改」错误。
    pub fn value_type_field_mutation(class_name: Identifier, field: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: ValueTypeErrorKind::ValueTypeFieldMutation { class_name: class_name.clone(), field: field.clone() },
            message: format!("值类型 {} 的字段 {} 不能被原地修改", class_name, field),
            span,
        }
    }
}

impl fmt::Display for ValueTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ValueTypeError {}

/// 收集模块内值类型并扫描其方法与 impl 中字段写入的检查器。
#[derive(Debug, Default)]
pub struct ValueTypeChecker {
    value_types: BTreeSet<Identifier>,
    errors: Vec<ValueTypeError>,
}

impl ValueTypeChecker {
    /// 创建空的值类型检查器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 检查模块内值类型的继承关系与字段写入并返回违规列表。
    pub fn check_module(&mut self, module: &HirModule) -> Vec<ValueTypeError> {
        self.clear();
        for class in &module.structs {
            if class.is_value_type {
                self.value_types.insert(class.name.clone());
            }
        }
        for class in &module.structs {
            if class.is_value_type {
                if let Some(parent) = class.parents.first().and_then(|parent| last_name(&parent.name)) {
                    if !self.value_types.contains(&parent) {
                        self.errors.push(ValueTypeError::value_type_inheritance(class.name.clone(), parent, None));
                    }
                }
            }
        }
        for class in &module.structs {
            if class.is_value_type {
                for method in &class.methods {
                    let mutations = self.scan_method_for_field_mutations(&class.name, method);
                    self.errors.extend(mutations);
                }
            }
        }
        for impl_block in &module.impls {
            if let HirType::Named(target_name) = &impl_block.target {
                if self.value_types.contains(target_name) {
                    for method in &impl_block.methods {
                        let mutations = self.scan_method_for_field_mutations(target_name, method);
                        self.errors.extend(mutations);
                    }
                }
            }
        }
        self.errors.clone()
    }

    fn scan_method_for_field_mutations(&self, class_name: &Identifier, method: &HirFunction) -> Vec<ValueTypeError> {
        let mut errors = Vec::new();
        let mut locals: BTreeMap<Identifier, Identifier> = BTreeMap::new();
        for param in &method.params {
            if let HirType::Named(ty_name) = &param.ty {
                if self.value_types.contains(ty_name) {
                    locals.insert(param.name.name.clone(), ty_name.clone());
                }
            }
        }
        self.scan_block(class_name, &method.body, &mut locals, &mut errors);
        errors
    }

    fn scan_block(
        &self,
        class_name: &Identifier,
        block: &HirBlock,
        locals: &mut BTreeMap<Identifier, Identifier>,
        errors: &mut Vec<ValueTypeError>,
    ) {
        for statement in &block.statements {
            self.scan_statement(class_name, statement, locals, errors);
        }
        if let Some(expr) = &block.expr {
            self.scan_expr(class_name, expr, locals, errors);
        }
    }

    fn scan_statement(
        &self,
        class_name: &Identifier,
        statement: &HirStatement,
        locals: &mut BTreeMap<Identifier, Identifier>,
        errors: &mut Vec<ValueTypeError>,
    ) {
        match &statement.kind {
            HirStatementKind::Let { pattern, initializer, ty, .. } => {
                let inferred_value_type = ty
                    .as_ref()
                    .and_then(|t| match t {
                        HirType::Named(name) if self.value_types.contains(name) => Some(name.clone()),
                        _ => None,
                    })
                    .or_else(|| {
                        initializer.as_ref().and_then(|init| match &init.kind {
                            HirExprKind::Construct { path, .. } => path.parts().last().filter(|name| self.value_types.contains(*name)).cloned(),
                            _ => None,
                        })
                    });
                if let Some(value_type_name) = inferred_value_type {
                    self.register_pattern_locals(pattern, &value_type_name, locals);
                }
                if let Some(init) = initializer {
                    self.scan_expr(class_name, init, locals, errors);
                }
            }
            HirStatementKind::Expr(expr) => {
                self.scan_expr(class_name, expr, locals, errors);
            }
        }
    }

    fn register_pattern_locals(&self, pattern: &HirPattern, value_type_name: &Identifier, locals: &mut BTreeMap<Identifier, Identifier>) {
        match pattern {
            HirPattern::Variable(id) => {
                locals.insert(id.name.clone(), value_type_name.clone());
            }
            HirPattern::Tuple(items) => {
                for item in items {
                    self.register_pattern_locals(item, value_type_name, locals);
                }
            }
            HirPattern::Bind { identifier, pattern } => {
                locals.insert(identifier.name.clone(), value_type_name.clone());
                self.register_pattern_locals(pattern, value_type_name, locals);
            }
            _ => {}
        }
    }

    fn scan_expr(
        &self,
        class_name: &Identifier,
        expr: &HirExpr,
        locals: &mut BTreeMap<Identifier, Identifier>,
        errors: &mut Vec<ValueTypeError>,
    ) {
        match &expr.kind {
            HirExprKind::Literal(_)
            | HirExprKind::Variable(_)
            | HirExprKind::Path(_)
            | HirExprKind::Continue { .. }
            | HirExprKind::Fallthrough => {}
            HirExprKind::StoreField { object, field, value, .. } => {
                if let Some(value_type_name) = self.resolve_object_value_type(class_name, object, locals) {
                    errors.push(ValueTypeError::value_type_field_mutation(value_type_name, field.clone(), Some(object.span.clone())));
                }
                self.scan_expr(class_name, object, locals, errors);
                self.scan_expr(class_name, value, locals, errors);
            }
            HirExprKind::Assign { value, .. } => {
                self.scan_expr(class_name, value, locals, errors);
            }
            HirExprKind::FieldAccess { object, .. } => {
                self.scan_expr(class_name, object, locals, errors);
            }
            HirExprKind::Call { callee, args, .. } => {
                self.scan_expr(class_name, callee, locals, errors);
                for arg in args {
                    self.scan_expr(class_name, &arg.value, locals, errors);
                }
            }
            HirExprKind::Construct { args, .. } => {
                for arg in args {
                    self.scan_expr(class_name, arg, locals, errors);
                }
            }
            HirExprKind::Cast { expr, .. } => {
                self.scan_expr(class_name, expr, locals, errors);
            }
            HirExprKind::FieldInit { value, .. } => {
                self.scan_expr(class_name, value, locals, errors);
            }
            HirExprKind::ArrayNew { length, .. } => {
                self.scan_expr(class_name, length, locals, errors);
            }
            HirExprKind::ArrayLiteral { items } | HirExprKind::TupleLiteral { items } => {
                for item in items {
                    self.scan_expr(class_name, item, locals, errors);
                }
            }
            HirExprKind::GenericApply { callee, .. } => {
                self.scan_expr(class_name, callee, locals, errors);
            }
            HirExprKind::Block(block) => {
                self.scan_block(class_name, block, locals, errors);
            }
            HirExprKind::Lambda { params, body, .. } => {
                let mut nested_locals = locals.clone();
                for param in params {
                    if let HirType::Named(ty_name) = &param.ty {
                        if self.value_types.contains(ty_name) {
                            nested_locals.insert(param.name.name.clone(), ty_name.clone());
                        }
                    }
                }
                self.scan_block(class_name, body, &mut nested_locals, errors);
            }
            HirExprKind::AnonymousClass { fields, .. } => {
                for (_, value) in fields {
                    self.scan_expr(class_name, value, locals, errors);
                }
            }
            HirExprKind::If { condition, then_branch, else_branch } => {
                self.scan_expr(class_name, condition, locals, errors);
                self.scan_block(class_name, then_branch, locals, errors);
                if let Some(else_block) = else_branch {
                    self.scan_block(class_name, else_block, locals, errors);
                }
            }
            HirExprKind::IfLet { scrutinee, then_branch, else_branch, .. } => {
                self.scan_expr(class_name, scrutinee, locals, errors);
                self.scan_block(class_name, then_branch, locals, errors);
                if let Some(else_block) = else_branch {
                    self.scan_block(class_name, else_block, locals, errors);
                }
            }
            HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
                self.scan_expr(class_name, scrutinee, locals, errors);
                for arm in arms {
                    if let Some(guard) = &arm.guard {
                        self.scan_expr(class_name, guard, locals, errors);
                    }
                    self.scan_expr(class_name, &arm.body, locals, errors);
                }
            }
            HirExprKind::Loop { iterator, condition, body, .. } => {
                if let Some(iter) = iterator {
                    self.scan_expr(class_name, iter, locals, errors);
                }
                if let Some(cond) = condition {
                    self.scan_expr(class_name, cond, locals, errors);
                }
                self.scan_block(class_name, body, locals, errors);
            }
            HirExprKind::Return(inner) => {
                if let Some(expr) = inner {
                    self.scan_expr(class_name, expr, locals, errors);
                }
            }
            HirExprKind::Break { expr, .. } => {
                if let Some(inner) = expr {
                    self.scan_expr(class_name, inner, locals, errors);
                }
            }
            HirExprKind::Yield(inner) => {
                if let Some(expr) = inner {
                    self.scan_expr(class_name, expr, locals, errors);
                }
            }
            HirExprKind::YieldFrom(expr)
            | HirExprKind::Await(expr)
            | HirExprKind::Awake(expr)
            | HirExprKind::BlockOn(expr)
            | HirExprKind::Raise(expr)
            | HirExprKind::Resume(expr)
            | HirExprKind::TryPropagate { inner: expr, .. } => {
                self.scan_expr(class_name, expr, locals, errors);
            }
            HirExprKind::Catch { expr, arms } => {
                self.scan_expr(class_name, expr, locals, errors);
                for arm in arms {
                    if let Some(guard) = &arm.guard {
                        self.scan_expr(class_name, guard, locals, errors);
                    }
                    self.scan_expr(class_name, &arm.body, locals, errors);
                }
            }
            HirExprKind::TryScope { body, .. } => {
                self.scan_block(class_name, body, locals, errors);
            }
            HirExprKind::With { base, updates } => {
                self.scan_expr(class_name, base, locals, errors);
                for (_, value) in updates {
                    self.scan_expr(class_name, value, locals, errors);
                }
            }
            HirExprKind::SuperCall { args, .. } => {
                for arg in args {
                    self.scan_expr(class_name, arg, locals, errors);
                }
            }
        }
    }

    fn resolve_object_value_type(
        &self,
        class_name: &Identifier,
        object: &HirExpr,
        locals: &BTreeMap<Identifier, Identifier>,
    ) -> Option<Identifier> {
        match &object.kind {
            HirExprKind::Variable(identifier) => {
                if let Some(value_type_name) = locals.get(&identifier.name) {
                    return Some(value_type_name.clone());
                }
                if identifier.name.as_str() == "self" {
                    return Some(class_name.clone());
                }
                None
            }
            HirExprKind::Path(path) => {
                if let Some(last) = path.parts().last() {
                    if let Some(value_type_name) = locals.get(last) {
                        return Some(value_type_name.clone());
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// 判断给定类型是否属于值语义类型（已登记值类型、元组、定长数组）。
    pub fn is_value_type(&self, ty: &HirType) -> bool {
        match ty {
            HirType::Named(name) => self.value_types.contains(name),
            HirType::Tuple(_) | HirType::FixedArray { .. } => true,
            _ => false,
        }
    }

    /// 返回已登记的全部值类型名。
    pub fn get_value_type_names(&self) -> Vec<Identifier> {
        self.value_types.iter().cloned().collect()
    }

    /// 返回已登记值类型的集合。
    pub fn value_types(&self) -> &BTreeSet<Identifier> {
        &self.value_types
    }

    /// 返回已收集的违规列表。
    pub fn errors(&self) -> &[ValueTypeError] {
        &self.errors
    }

    /// 清空已登记值类型与违规列表。
    pub fn clear(&mut self) {
        self.value_types.clear();
        self.errors.clear();
    }
}
