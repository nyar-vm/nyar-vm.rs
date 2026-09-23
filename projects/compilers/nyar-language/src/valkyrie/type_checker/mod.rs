#![allow(missing_docs)]

mod class_semantics;
mod composite_pattern_exhaustiveness;
mod control_flow_validation;
mod effect_typing;
mod imply_validation;
mod literal_exhaustiveness;
mod oop_checks;
mod pattern_checks;
mod singleton_checks;
mod unreachable_arm;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use crate::types::{
    Identifier, NamePath, SourceSpan,
    hir::{
        AccessLevel, HirBlock, HirExpr, HirExprKind, HirField, HirFunction, HirImpl, HirModule, HirPattern, HirProperty, HirStatement,
        HirStatementKind, HirStruct, HirWidget, ValkyrieType as HirType,
    },
};

pub use self::{
    class_semantics::{
        AbstractClassChecker, AbstractClassError, AbstractClassErrorKind, FinalClassChecker, FinalClassError, FinalClassErrorKind,
    },
    composite_pattern_exhaustiveness::{
        CompositePatternExhaustivenessChecker, CompositePatternExhaustivenessError, CompositePatternExhaustivenessErrorKind,
    },
    control_flow_validation::{InferenceTypeVar, TypeError, TypeInference},
    effect_typing::{
        AssociatedTypeConstraint, ConstraintChainNode, ConstraintError, ConstraintErrorKind, ConstraintPropagator, ConstraintReport,
        ConstraintSolver, FixSuggestion, Lifetime, LifetimeConstraint, LifetimeConstraintKind, MultiTraitBound, TraitBoundChecker, TraitImpl,
        TypeConstraint, TypeVar, WhereBound, WhereClause,
    },
    imply_validation::{ImplyChecker, ImplyError, ImplyErrorKind},
    literal_exhaustiveness::{LiteralExhaustivenessChecker, LiteralExhaustivenessError, LiteralExhaustivenessErrorKind},
    oop_checks::SealedMatchChecker,
    pattern_checks::check_pattern_refutability,
    singleton_checks::{SingletonChecker, SingletonError, SingletonErrorKind},
    unreachable_arm::{UnreachableArmChecker, UnreachableArmError, UnreachableArmErrorKind},
};

fn display_path(path: &NamePath) -> String {
    path.parts().iter().map(|part| part.to_string()).collect::<Vec<_>>().join("::")
}

fn last_name(path: &NamePath) -> Option<Identifier> {
    path.parts().last().cloned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessContext {
    pub current_module: NamePath,
    pub current_class: Option<Identifier>,
    pub current_method: Option<Identifier>,
}

impl AccessContext {
    pub fn new(current_module: NamePath) -> Self {
        Self { current_module, current_class: None, current_method: None }
    }

    pub fn in_class(current_module: NamePath, current_class: Identifier) -> Self {
        Self { current_module, current_class: Some(current_class), current_method: None }
    }

    pub fn in_method(current_module: NamePath, current_class: Identifier, current_method: Identifier) -> Self {
        Self { current_module, current_class: Some(current_class), current_method: Some(current_method) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessControlErrorKind {
    PrivateMemberAccess { owner: Identifier, member: Identifier, accessor: Option<Identifier> },
    ProtectedMemberAccess { owner: Identifier, member: Identifier, accessor: Identifier },
    InternalMemberAccess { member: Identifier, declared_module: NamePath, current_module: NamePath },
    ReadonlyFieldWrite { owner: Identifier, field: Identifier },
    PrivateConstructorInstantiation { class_name: Identifier },
    VisibilityReduction { class_name: Identifier, member: Identifier, from: AccessLevel, to: AccessLevel },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessControlError {
    pub kind: AccessControlErrorKind,
    pub message: String,
    pub span: Option<SourceSpan>,
}

impl AccessControlError {
    pub fn private_member_access(owner: Identifier, member: Identifier, accessor: Option<Identifier>, span: Option<SourceSpan>) -> Self {
        let accessor_text = accessor.as_ref().map(|name| format!(" by {}", name)).unwrap_or_default();
        Self {
            kind: AccessControlErrorKind::PrivateMemberAccess { owner: owner.clone(), member: member.clone(), accessor },
            message: format!("private member access: {}.{}{}", owner, member, accessor_text),
            span,
        }
    }

    pub fn protected_member_access(owner: Identifier, member: Identifier, accessor: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: AccessControlErrorKind::ProtectedMemberAccess { owner: owner.clone(), member: member.clone(), accessor: accessor.clone() },
            message: format!("protected member access denied: {}.{} from {}", owner, member, accessor),
            span,
        }
    }

    pub fn internal_member_access(member: Identifier, declared_module: NamePath, current_module: NamePath, span: Option<SourceSpan>) -> Self {
        Self {
            kind: AccessControlErrorKind::InternalMemberAccess {
                member: member.clone(),
                declared_module: declared_module.clone(),
                current_module: current_module.clone(),
            },
            message: format!(
                "internal member access denied: {} declared in {} from {}",
                member,
                display_path(&declared_module),
                display_path(&current_module)
            ),
            span,
        }
    }

    pub fn readonly_field_write(owner: Identifier, field: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: AccessControlErrorKind::ReadonlyFieldWrite { owner: owner.clone(), field: field.clone() },
            message: format!("readonly field write denied: {}.{}", owner, field),
            span,
        }
    }

    pub fn private_constructor_instantiation(class_name: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: AccessControlErrorKind::PrivateConstructorInstantiation { class_name: class_name.clone() },
            message: format!("private constructor instantiation denied: {} requires a factory method", class_name),
            span,
        }
    }

    pub fn visibility_reduction(
        class_name: Identifier,
        member: Identifier,
        from: AccessLevel,
        to: AccessLevel,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            kind: AccessControlErrorKind::VisibilityReduction { class_name: class_name.clone(), member: member.clone(), from, to },
            message: format!("reduced visibility is not allowed: {}.{} from {} to {}", class_name, member, from.as_str(), to.as_str()),
            span,
        }
    }
}

impl fmt::Display for AccessControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AccessControlError {}

#[derive(Debug, Default)]
pub struct AccessControlChecker {
    errors: Vec<AccessControlError>,
    classes: BTreeMap<Identifier, HirStruct>,
    inheritance_map: BTreeMap<Identifier, Vec<Identifier>>,
}

impl AccessControlChecker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn errors(&self) -> &[AccessControlError] {
        &self.errors
    }

    pub fn classes(&self) -> &BTreeMap<Identifier, HirStruct> {
        &self.classes
    }

    pub fn inheritance_map(&self) -> &BTreeMap<Identifier, Vec<Identifier>> {
        &self.inheritance_map
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstructorChainErrorKind {
    MissingSuperCall { class_name: Identifier, parent: Identifier },
    SuperCallArgumentMismatch { class_name: Identifier, parent: Identifier, expected: usize, got: usize },
    InvalidSuperCallOrder { class_name: Identifier, expected: Vec<Identifier>, actual: Vec<Identifier> },
    DuplicateSuperCall { class_name: Identifier, parent: Identifier },
    InvalidSuperCallMethod { class_name: Identifier, method: Identifier },
    SuperCallArgumentTypeMismatch { class_name: Identifier, parent: Identifier, index: usize, expected: HirType, got: HirType },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstructorChainError {
    pub kind: ConstructorChainErrorKind,
    pub message: String,
    pub span: Option<SourceSpan>,
}

impl ConstructorChainError {
    pub fn missing_super_call(class_name: Identifier, parent: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: ConstructorChainErrorKind::MissingSuperCall { class_name: class_name.clone(), parent: parent.clone() },
            message: format!("{} is missing required super call to {}", class_name, parent),
            span,
        }
    }

    pub fn super_call_argument_mismatch(
        class_name: Identifier,
        parent: Identifier,
        expected: usize,
        got: usize,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            kind: ConstructorChainErrorKind::SuperCallArgumentMismatch {
                class_name: class_name.clone(),
                parent: parent.clone(),
                expected,
                got,
            },
            message: format!("Expected {} arguments for {} super call in {}, got {}", expected, parent, class_name, got),
            span,
        }
    }

    pub fn invalid_super_call_order(
        class_name: Identifier,
        expected: Vec<Identifier>,
        actual: Vec<Identifier>,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            kind: ConstructorChainErrorKind::InvalidSuperCallOrder {
                class_name: class_name.clone(),
                expected: expected.clone(),
                actual: actual.clone(),
            },
            message: format!("Invalid MRO super call order in {}", class_name),
            span,
        }
    }

    pub fn duplicate_super_call(class_name: Identifier, parent: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: ConstructorChainErrorKind::DuplicateSuperCall { class_name: class_name.clone(), parent: parent.clone() },
            message: format!("Duplicate super call to {} in {}", parent, class_name),
            span,
        }
    }

    pub fn invalid_super_call_method(class_name: Identifier, method: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: ConstructorChainErrorKind::InvalidSuperCallMethod { class_name: class_name.clone(), method: method.clone() },
            message: format!("{} may only call super from initiate, not {}", class_name, method),
            span,
        }
    }

    pub fn super_call_argument_type_mismatch(
        class_name: Identifier,
        parent: Identifier,
        index: usize,
        expected: HirType,
        got: HirType,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            kind: ConstructorChainErrorKind::SuperCallArgumentTypeMismatch {
                class_name: class_name.clone(),
                parent: parent.clone(),
                index,
                expected: expected.clone(),
                got: got.clone(),
            },
            message: format!(
                "super call argument type mismatch in {} -> {} at {}: expected {:?}, got {:?}",
                class_name, parent, index, expected, got
            ),
            span,
        }
    }
}

impl fmt::Display for ConstructorChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ConstructorChainError {}

#[derive(Debug, Default)]
pub struct ConstructorChainChecker {
    errors: Vec<ConstructorChainError>,
}

impl ConstructorChainChecker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn errors(&self) -> &[ConstructorChainError] {
        &self.errors
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyErrorKind {
    VirtualStaticConflict { property: Identifier },
    StaticWithSelf { property: Identifier },
    LazyPropertyWithSetter { property: Identifier },
    InvalidOverride { class_name: Identifier, property: Identifier },
    AbstractPropertyWithBody { class_name: Identifier, property: Identifier },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyError {
    pub kind: PropertyErrorKind,
    pub message: String,
    pub span: Option<SourceSpan>,
}

impl PropertyError {
    pub fn virtual_static_conflict(property: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: PropertyErrorKind::VirtualStaticConflict { property: property.clone() },
            message: format!("property {} cannot be both virtual and static", property),
            span,
        }
    }

    pub fn static_with_self(property: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: PropertyErrorKind::StaticWithSelf { property: property.clone() },
            message: format!("static property {} cannot use self", property),
            span,
        }
    }

    pub fn lazy_with_setter(property: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: PropertyErrorKind::LazyPropertyWithSetter { property: property.clone() },
            message: format!("lazy property {} cannot define a setter", property),
            span,
        }
    }

    pub fn invalid_override(class_name: Identifier, property: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: PropertyErrorKind::InvalidOverride { class_name: class_name.clone(), property: property.clone() },
            message: format!("property {} in {} cannot override without a parent", property, class_name),
            span,
        }
    }

    pub fn abstract_property_with_body(class_name: Identifier, property: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: PropertyErrorKind::AbstractPropertyWithBody { class_name: class_name.clone(), property: property.clone() },
            message: format!("abstract property {} in {} cannot have a concrete body", property, class_name),
            span,
        }
    }
}

impl fmt::Display for PropertyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for PropertyError {}

#[derive(Debug, Default)]
pub struct PropertyChecker {
    errors: Vec<PropertyError>,
}

impl PropertyChecker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check_module(&mut self, module: &HirModule) -> Vec<PropertyError> {
        self.errors.clear();
        for class in &module.structs {
            for property in &class.properties {
                self.check_property(class, property);
            }
        }
        self.errors.clone()
    }

    fn check_property(&mut self, class: &HirStruct, property: &HirProperty) {
        if property.is_static && property.is_virtual {
            self.errors.push(PropertyError::virtual_static_conflict(property.name.clone(), None));
        }
        if property.is_lazy && property.setter.is_some() {
            self.errors.push(PropertyError::lazy_with_setter(property.name.clone(), None));
        }
        if property.is_override && class.parents.is_empty() {
            self.errors.push(PropertyError::invalid_override(class.name.clone(), property.name.clone(), None));
        }
        if property.is_abstract && !class.is_abstract {
            self.errors.push(PropertyError::abstract_property_with_body(class.name.clone(), property.name.clone(), None));
        }
    }
}

#[derive(Debug, Default)]
pub struct SetterValidationAnalyzer;

impl SetterValidationAnalyzer {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze(&mut self, property: &HirProperty) -> Vec<String> {
        vec![format!("validate setter for {}", property.name)]
    }

    pub fn generate_panic_code(property_name: &str, condition: &str) -> String {
        format!("if !({}) {{ panic!(\"invalid setter value for {}\"); }}", condition, property_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueTypeErrorKind {
    ValueTypeInheritance { class_name: Identifier, parent: Identifier },
    ValueTypeFieldMutation { class_name: Identifier, field: Identifier },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueTypeError {
    pub kind: ValueTypeErrorKind,
    pub message: String,
    pub span: Option<SourceSpan>,
}

impl ValueTypeError {
    pub fn value_type_inheritance(class_name: Identifier, parent: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: ValueTypeErrorKind::ValueTypeInheritance { class_name: class_name.clone(), parent: parent.clone() },
            message: format!("值类型 {} 不能继承 {}", class_name, parent),
            span,
        }
    }

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

#[derive(Debug, Default)]
pub struct ValueTypeChecker {
    value_types: BTreeSet<Identifier>,
    errors: Vec<ValueTypeError>,
}

impl ValueTypeChecker {
    pub fn new() -> Self {
        Self::default()
    }

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
                            HirExprKind::Construct { name, .. } if self.value_types.contains(name) => Some(name.clone()),
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
            HirExprKind::StoreField { object, field, value } => {
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
            HirExprKind::FieldInit { value, .. } => {
                self.scan_expr(class_name, value, locals, errors);
            }
            HirExprKind::ArrayNew { length, .. } => {
                self.scan_expr(class_name, length, locals, errors);
            }
            HirExprKind::ArrayLiteral { items } => {
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
            | HirExprKind::TryPropagate(expr) => {
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

    pub fn is_value_type(&self, ty: &HirType) -> bool {
        match ty {
            HirType::Named(name) => self.value_types.contains(name),
            HirType::Tuple(_) | HirType::FixedArray { .. } => true,
            _ => false,
        }
    }

    pub fn get_value_type_names(&self) -> Vec<Identifier> {
        self.value_types.iter().cloned().collect()
    }

    pub fn value_types(&self) -> &BTreeSet<Identifier> {
        &self.value_types
    }

    pub fn errors(&self) -> &[ValueTypeError] {
        &self.errors
    }

    pub fn clear(&mut self) {
        self.value_types.clear();
        self.errors.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentSemantics {
    Copy,
    Reference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterSemantics {
    Copy,
    Reference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnSemantics {
    Copy,
    Reference,
}

#[derive(Debug, Default)]
pub struct CopySemanticsValidator {
    value_types: BTreeSet<Identifier>,
}

impl CopySemanticsValidator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_value_type(&mut self, class: &HirStruct) {
        if class.is_value_type {
            self.value_types.insert(class.name.clone());
        }
    }

    fn is_value(&self, ty: &HirType) -> bool {
        match ty {
            HirType::Named(name) => self.value_types.contains(name),
            HirType::Tuple(_) | HirType::FixedArray { .. } => true,
            _ => false,
        }
    }

    pub fn validate_assignment(&self, ty: &HirType) -> AssignmentSemantics {
        if self.is_value(ty) { AssignmentSemantics::Copy } else { AssignmentSemantics::Reference }
    }

    pub fn validate_parameter_passing(&self, ty: &HirType) -> ParameterSemantics {
        if self.is_value(ty) { ParameterSemantics::Copy } else { ParameterSemantics::Reference }
    }

    pub fn validate_return(&self, ty: &HirType) -> ReturnSemantics {
        if self.is_value(ty) { ReturnSemantics::Copy } else { ReturnSemantics::Reference }
    }

    /// 判断给定类型是否为已注册的值类型或内建值语义类型（元组、定长数组）。
    pub fn is_value_type(&self, ty: &HirType) -> bool {
        self.is_value(ty)
    }

    /// 判断给定类型是否可被值类型安全地按拷贝语义持有。
    ///
    /// 仅 copy 兼容类型可作为值类型 struct 的字段：基本标量、定长数组、元组、
    /// 以及已注册的值类型。堆数组、函数、trait 对象、未注册命名类型等引用类型
    /// 不兼容，违反 copy discipline。
    fn is_copy_compatible(&self, ty: &HirType) -> bool {
        match ty {
            HirType::Void
            | HirType::Unit
            | HirType::Boolean
            | HirType::Character
            | HirType::Utf8
            | HirType::Utf16
            | HirType::Float32
            | HirType::Float64
            | HirType::AutoType
            | HirType::SelfType => true,
            HirType::Integer8 { .. }
            | HirType::Integer16 { .. }
            | HirType::Integer32 { .. }
            | HirType::Integer64 { .. }
            | HirType::Integer128 { .. } => true,
            HirType::Tuple(items) => items.iter().all(|item| self.is_copy_compatible(item)),
            HirType::FixedArray { element, .. } => self.is_copy_compatible(element),
            HirType::Named(name) => self.value_types.contains(name),
            HirType::Array(_)
            | HirType::Function(_)
            | HirType::TypeLambda(_)
            | HirType::TraitObject(_)
            | HirType::Associated(_)
            | HirType::Generic(_)
            | HirType::Row(_)
            | HirType::Apply(_, _)
            | HirType::Nullable(_)
            | HirType::Union(_)
            | HirType::Intersection(_) => false,
        }
    }

    /// 在值类型结构体中查找首个引用类型字段，返回该字段引用以便定位违规。
    ///
    /// 值类型应当只包含拷贝兼容的字段；若存在堆数组或未注册命名类型等引用字段，
    /// 则该值类型无法被安全地按拷贝语义传递或返回，违反 copy discipline。
    pub fn find_non_copy_field<'a>(&self, struct_def: &'a HirStruct) -> Option<&'a HirField> {
        struct_def.fields.iter().find(|field| !self.is_copy_compatible(&field.ty))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealedClassErrorKind {
    NonExhaustiveMatch,
    UnknownSealedClass,
    NotSealedBase,
    DuplicateMatchArm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedClassError {
    pub kind: SealedClassErrorKind,
    pub message: String,
    pub span: Option<SourceSpan>,
}

impl fmt::Display for SealedClassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SealedClassError {}

#[derive(Debug, Default, Clone)]
pub struct SealedClassRegistry {
    sealed_classes: BTreeSet<Identifier>,
    subclasses: BTreeMap<Identifier, BTreeSet<Identifier>>,
}

impl SealedClassRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_sealed_class(&mut self, class: &HirStruct) {
        if class.is_sealed {
            self.sealed_classes.insert(class.name.clone());
        }
    }

    pub fn register_subclass(&mut self, sealed_name: &Identifier, subclass: &Identifier) -> Result<(), SealedClassError> {
        if !self.sealed_classes.contains(sealed_name) {
            return Err(SealedClassError {
                kind: SealedClassErrorKind::NotSealedBase,
                message: format!("{} is not a sealed class", sealed_name),
                span: None,
            });
        }
        self.subclasses.entry(sealed_name.clone()).or_default().insert(subclass.clone());
        Ok(())
    }

    pub fn is_sealed_class(&self, name: &Identifier) -> bool {
        self.sealed_classes.contains(name)
    }

    pub fn get_permitted_subclasses(&self, name: &Identifier) -> Vec<Identifier> {
        self.subclasses.get(name).map(|items| items.iter().cloned().collect()).unwrap_or_default()
    }

    pub fn sealed_class_names(&self) -> impl Iterator<Item = &Identifier> {
        self.sealed_classes.iter()
    }
}

#[derive(Debug, Default, Clone)]
pub struct EnumRegistry {
    sum_types: BTreeMap<Identifier, Vec<Identifier>>,
}

impl EnumRegistry {
    pub fn from_module(module: &HirModule) -> Self {
        let mut registry = Self::default();
        for enum_def in module.enums.iter().chain(module.imported_enums.iter()).chain(module.imported_nominal_enums()) {
            registry.sum_types.insert(enum_def.name.clone(), enum_def.variants.iter().map(|variant| variant.name.clone()).collect());
        }
        registry
    }

    pub fn is_sum_type(&self, name: &Identifier) -> bool {
        self.sum_types.contains_key(name)
    }

    pub fn variants(&self, name: &Identifier) -> &[Identifier] {
        self.sum_types.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn enum_from_covered_variants(&self, covered: &[Identifier]) -> Option<Identifier> {
        let mut candidates = BTreeSet::new();
        for (enum_name, variants) in &self.sum_types {
            if covered.iter().any(|variant| variants.contains(variant)) {
                candidates.insert(enum_name.clone());
            }
        }
        if candidates.len() == 1 { candidates.into_iter().next() } else { None }
    }
}

#[derive(Debug, Clone)]
pub struct ExhaustivenessChecker {
    registry: SealedClassRegistry,
    enum_registry: EnumRegistry,
}

impl ExhaustivenessChecker {
    pub fn new(registry: SealedClassRegistry) -> Self {
        Self { registry, enum_registry: EnumRegistry::default() }
    }

    pub fn with_enum_registry(registry: SealedClassRegistry, enum_registry: EnumRegistry) -> Self {
        Self { registry, enum_registry }
    }

    pub fn enum_registry(&self) -> &EnumRegistry {
        &self.enum_registry
    }

    pub fn sealed_registry(&self) -> &SealedClassRegistry {
        &self.registry
    }

    pub fn check_exhaustiveness(&self, sealed_name: &Identifier, covered: &[Identifier]) -> Result<(), SealedClassError> {
        let declared = self.registry.get_permitted_subclasses(sealed_name);
        let covered_set = covered.iter().cloned().collect::<BTreeSet<_>>();
        let missing = declared.into_iter().filter(|name| !covered_set.contains(name)).collect::<Vec<_>>();
        if missing.is_empty() {
            Ok(())
        }
        else {
            Err(SealedClassError {
                kind: SealedClassErrorKind::NonExhaustiveMatch,
                message: format!(
                    "non exhaustive match for {}: missing {}",
                    sealed_name,
                    missing.iter().map(|item| item.to_string()).collect::<Vec<_>>().join(", ")
                ),
                span: None,
            })
        }
    }

    pub fn is_wildcard_exhaustive(&self, _type_name: &Identifier, has_wildcard: bool) -> bool {
        has_wildcard
    }

    pub fn check_duplicate_arms(&self, covered: &[Identifier]) -> Option<SealedClassError> {
        let mut seen = BTreeSet::new();
        for variant in covered {
            if !seen.insert(variant.clone()) {
                return Some(SealedClassError {
                    kind: SealedClassErrorKind::DuplicateMatchArm,
                    message: format!("duplicate match arm for {}", variant),
                    span: None,
                });
            }
        }
        None
    }

    pub fn check_variant_exhaustiveness(&self, enum_name: &Identifier, covered: &[Identifier]) -> Result<(), SealedClassError> {
        let declared = self.enum_registry.variants(enum_name);
        let covered_set = covered.iter().cloned().collect::<BTreeSet<_>>();
        let missing = declared.iter().filter(|variant| !covered_set.contains(*variant)).cloned().collect::<Vec<_>>();
        if missing.is_empty() {
            Ok(())
        }
        else {
            Err(SealedClassError {
                kind: SealedClassErrorKind::NonExhaustiveMatch,
                message: format!(
                    "non exhaustive match for {}: missing {}",
                    enum_name,
                    missing.iter().map(|item| item.to_string()).collect::<Vec<_>>().join(", ")
                ),
                span: None,
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WidgetErrorKind {
    MissingRenderMethod { widget: Identifier },
    InvalidRenderReturnType { widget: Identifier, found: HirType },
    InvalidStateUpdate { widget: Identifier, field: Identifier },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidgetError {
    pub kind: WidgetErrorKind,
    pub message: String,
    pub span: Option<SourceSpan>,
}

impl WidgetError {
    pub fn missing_render_method(widget: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: WidgetErrorKind::MissingRenderMethod { widget: widget.clone() },
            message: format!("widget {} is missing render method", widget),
            span,
        }
    }

    pub fn invalid_render_return_type(widget: Identifier, found: HirType, span: Option<SourceSpan>) -> Self {
        Self {
            kind: WidgetErrorKind::InvalidRenderReturnType { widget: widget.clone(), found: found.clone() },
            message: format!("widget {} render must return Element, found {:?}", widget, found),
            span,
        }
    }

    pub fn invalid_state_update(widget: Identifier, field: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: WidgetErrorKind::InvalidStateUpdate { widget: widget.clone(), field: field.clone() },
            message: format!("widget {} has invalid state update for {}", widget, field),
            span,
        }
    }
}

impl fmt::Display for WidgetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for WidgetError {}

#[derive(Debug, Default)]
pub struct WidgetChecker {
    widgets: BTreeMap<Identifier, HirWidget>,
    errors: Vec<WidgetError>,
    current_widget: Option<Identifier>,
    in_event_handler: bool,
    in_lifecycle_method: bool,
}

impl WidgetChecker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn collect_widgets(&mut self, module: &HirModule) {
        for widget in &module.widgets {
            self.widgets.insert(widget.name.clone(), widget.clone());
        }
        for submodule in &module.submodules {
            self.collect_widgets(submodule);
        }
    }

    pub fn check_module(&mut self, module: &HirModule) -> Vec<WidgetError> {
        self.clear();
        self.collect_widgets(module);
        for widget in module.widgets.iter().chain(module.submodules.iter().flat_map(|module| module.widgets.iter())) {
            self.current_widget = Some(widget.name.clone());
            if let Some(render) = self.find_render_method(widget) {
                if !self.is_element_type(&render.return_type) {
                    self.errors.push(WidgetError::invalid_render_return_type(widget.name.clone(), render.return_type.clone(), None));
                }
            }
            else {
                self.errors.push(WidgetError::missing_render_method(widget.name.clone(), None));
            }
        }
        self.current_widget = None;
        self.errors.clone()
    }

    pub fn is_element_type(&self, ty: &HirType) -> bool {
        matches!(ty, HirType::Named(name) if name == &Identifier::new("Element"))
    }

    pub fn is_event_handler_method(&self, name: &str) -> bool {
        name.starts_with("on") && !matches!(name, "on_mount" | "on_unmount" | "on_update" | "before_update" | "after_update")
    }

    pub fn is_lifecycle_method(&self, name: &str) -> bool {
        matches!(name, "on_mount" | "on_unmount" | "on_update" | "before_update" | "after_update")
    }

    pub fn is_state_field(&self, name: &Identifier) -> bool {
        let text = name.as_str();
        text.starts_with('_') || text.starts_with("state_")
    }

    pub fn is_valid_state_update_context(&self) -> bool {
        self.in_event_handler || self.in_lifecycle_method
    }

    pub fn set_in_event_handler(&mut self, value: bool) {
        self.in_event_handler = value;
    }

    pub fn set_in_lifecycle_method(&mut self, value: bool) {
        self.in_lifecycle_method = value;
    }

    pub fn get_widget_names(&self) -> Vec<Identifier> {
        self.widgets.keys().cloned().collect()
    }

    pub fn widgets(&self) -> &BTreeMap<Identifier, HirWidget> {
        &self.widgets
    }

    pub fn errors(&self) -> &[WidgetError] {
        &self.errors
    }

    pub fn current_widget(&self) -> Option<&Identifier> {
        self.current_widget.as_ref()
    }

    pub fn clear(&mut self) {
        self.widgets.clear();
        self.errors.clear();
        self.current_widget = None;
        self.in_event_handler = false;
        self.in_lifecycle_method = false;
    }

    pub fn find_render_method<'a>(&self, widget: &'a HirWidget) -> Option<&'a HirFunction> {
        widget.methods.iter().find(|method| matches!(method.name.as_str(), "render" | "view"))
    }
}
