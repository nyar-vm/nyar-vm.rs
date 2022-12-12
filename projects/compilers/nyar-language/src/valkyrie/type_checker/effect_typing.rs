use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use crate::types::{
    Identifier, NamePath, SourceSpan,
    hir::{AssociatedType, FunctionType, HirEnum, ValkyrieType as HirType},
};

use super::display_path;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeVar(pub usize);

impl fmt::Display for TypeVar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "?T{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lifetime(String);

impl Lifetime {
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
    }

    pub fn static_lifetime() -> Self {
        Self("static".to_string())
    }
}

impl fmt::Display for Lifetime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "'{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeConstraint {
    TraitBound { var: TypeVar, trait_name: NamePath, span: Option<SourceSpan> },
    Equality { left: HirType, right: HirType, span: Option<SourceSpan> },
    Subtype { sub: HirType, sup: HirType, span: Option<SourceSpan> },
}

impl TypeConstraint {
    pub fn trait_bound(var: TypeVar, trait_name: NamePath, span: Option<SourceSpan>) -> Self {
        Self::TraitBound { var, trait_name, span }
    }

    pub fn equality(left: HirType, right: HirType, span: Option<SourceSpan>) -> Self {
        Self::Equality { left, right, span }
    }

    pub fn subtype(sub: HirType, sup: HirType, span: Option<SourceSpan>) -> Self {
        Self::Subtype { sub, sup, span }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitImpl {
    pub ty: HirType,
    pub trait_name: NamePath,
    pub type_args: Vec<HirType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiTraitBound {
    pub var: TypeVar,
    pub traits: Vec<NamePath>,
    pub span: Option<SourceSpan>,
}

impl MultiTraitBound {
    pub fn new(var: TypeVar, traits: Vec<NamePath>, span: Option<SourceSpan>) -> Self {
        Self { var, traits, span }
    }

    pub fn is_empty(&self) -> bool {
        self.traits.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhereBound {
    pub ty: HirType,
    pub traits: Vec<NamePath>,
    pub span: Option<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhereClause {
    pub bounds: Vec<WhereBound>,
    pub span: Option<SourceSpan>,
}

impl WhereClause {
    pub fn new(bounds: Vec<WhereBound>, span: Option<SourceSpan>) -> Self {
        Self { bounds, span }
    }

    pub fn empty() -> Self {
        Self { bounds: vec![], span: None }
    }

    pub fn add_bound(&mut self, bound: WhereBound) {
        self.bounds.push(bound);
    }

    pub fn is_empty(&self) -> bool {
        self.bounds.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifetimeConstraintKind {
    Outlives { longer: Lifetime, shorter: Lifetime },
    Equality { left: Lifetime, right: Lifetime },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifetimeConstraint {
    pub kind: LifetimeConstraintKind,
    pub span: Option<SourceSpan>,
}

impl LifetimeConstraint {
    pub fn outlives(longer: Lifetime, shorter: Lifetime, span: Option<SourceSpan>) -> Self {
        Self { kind: LifetimeConstraintKind::Outlives { longer, shorter }, span }
    }

    pub fn equality(left: Lifetime, right: Lifetime, span: Option<SourceSpan>) -> Self {
        Self { kind: LifetimeConstraintKind::Equality { left, right }, span }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociatedTypeConstraint {
    pub base_type: HirType,
    pub trait_name: NamePath,
    pub assoc_name: Identifier,
    pub expected: HirType,
    pub span: Option<SourceSpan>,
}

impl AssociatedTypeConstraint {
    pub fn new(base_type: HirType, trait_name: NamePath, assoc_name: Identifier, expected: HirType, span: Option<SourceSpan>) -> Self {
        Self { base_type, trait_name, assoc_name, expected, span }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstraintErrorKind {
    TraitNotImplemented { type_name: String, trait_name: String },
    TypeMismatch { expected: HirType, found: HirType },
    InfiniteType { var: TypeVar, ty: HirType },
    AmbiguousType { var: TypeVar },
    AssociatedTypeNotFound { type_name: String, trait_name: String, assoc_name: String },
    PropagationFailed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintError {
    pub kind: ConstraintErrorKind,
    pub message: String,
    pub span: Option<SourceSpan>,
}

impl ConstraintError {
    pub fn trait_not_implemented(type_name: &str, trait_name: &str, span: Option<SourceSpan>) -> Self {
        Self {
            kind: ConstraintErrorKind::TraitNotImplemented { type_name: type_name.to_string(), trait_name: trait_name.to_string() },
            message: format!("类型 {} 未实现 trait {}", type_name, trait_name),
            span,
        }
    }

    pub fn type_mismatch(expected: HirType, found: HirType, span: Option<SourceSpan>) -> Self {
        Self {
            kind: ConstraintErrorKind::TypeMismatch { expected: expected.clone(), found: found.clone() },
            message: format!("类型不匹配: expected {:?}, found {:?}", expected, found),
            span,
        }
    }

    pub fn infinite_type(var: TypeVar, ty: HirType, span: Option<SourceSpan>) -> Self {
        Self {
            kind: ConstraintErrorKind::InfiniteType { var: var.clone(), ty: ty.clone() },
            message: format!("无限类型: {} occurs in {:?}", var, ty),
            span,
        }
    }

    pub fn ambiguous_type(var: TypeVar, span: Option<SourceSpan>) -> Self {
        Self { kind: ConstraintErrorKind::AmbiguousType { var: var.clone() }, message: format!("类型歧义: {}", var), span }
    }

    pub fn associated_type_not_found(base_type: HirType, trait_name: NamePath, assoc_name: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: ConstraintErrorKind::AssociatedTypeNotFound {
                type_name: format!("{:?}", base_type),
                trait_name: display_path(&trait_name),
                assoc_name: assoc_name.to_string(),
            },
            message: "关联类型未找到".to_string(),
            span,
        }
    }

    pub fn propagation_failed(reason: &str, span: Option<SourceSpan>) -> Self {
        Self {
            kind: ConstraintErrorKind::PropagationFailed { reason: reason.to_string() }, message: format!("约束传播失败: {}", reason), span
        }
    }
}

impl fmt::Display for ConstraintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ConstraintError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixSuggestion {
    pub message: String,
    pub priority: i32,
    pub code_example: Option<String>,
}

impl FixSuggestion {
    pub fn new(message: impl Into<String>, priority: i32) -> Self {
        Self { message: message.into(), priority, code_example: None }
    }

    pub fn with_code_example(mut self, code_example: impl Into<String>) -> Self {
        self.code_example = Some(code_example.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintChainNode {
    pub description: String,
    pub span: Option<SourceSpan>,
    pub origin: Option<String>,
}

impl ConstraintChainNode {
    pub fn new(description: String, span: Option<SourceSpan>, origin: Option<String>) -> Self {
        Self { description, span, origin }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintReport {
    pub kind: ConstraintErrorKind,
    pub span: Option<SourceSpan>,
    pub primary_message: String,
    pub suggestions: Vec<FixSuggestion>,
    pub related_types: Vec<String>,
    pub chain: Vec<ConstraintChainNode>,
}

impl ConstraintReport {
    pub fn new(kind: ConstraintErrorKind, span: Option<SourceSpan>, primary_message: impl Into<String>) -> Self {
        Self { kind, span, primary_message: primary_message.into(), suggestions: vec![], related_types: vec![], chain: vec![] }
    }

    pub fn add_chain_node(&mut self, node: ConstraintChainNode) {
        self.chain.push(node);
    }

    pub fn add_suggestion(&mut self, suggestion: FixSuggestion) {
        self.suggestions.push(suggestion);
    }

    pub fn to_detailed_message(&self) -> String {
        format!(
            r#"错误: {}
约束追溯: {}
建议修复: {}"#,
            self.primary_message,
            self.chain.len(),
            self.suggestions.len()
        )
    }
}

#[derive(Debug, Default)]
pub struct TraitBoundChecker {
    trait_impls: Vec<TraitImpl>,
}

impl TraitBoundChecker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_builtin_trait(&self, trait_name: &NamePath) -> bool {
        matches!(display_path(trait_name).as_str(), "Copy" | "Clone" | "Eq" | "Send" | "Sync" | "Default")
    }

    pub fn register_trait_impl(&mut self, item: TraitImpl) {
        self.trait_impls.push(item);
    }

    pub fn check_trait_bound(&self, ty: &HirType, trait_name: &NamePath) -> Result<bool, ConstraintError> {
        if self.is_builtin_trait(trait_name)
            && matches!(
                ty,
                HirType::Integer32 { signed: _ } | HirType::Integer64 { signed: _ } | HirType::Float32 | HirType::Float64 | HirType::Boolean
            )
        {
            return Ok(true);
        }
        Ok(self.trait_impls.iter().any(|item| &item.ty == ty && item.trait_name == *trait_name))
    }
}

#[derive(Debug, Default)]
pub struct ConstraintPropagator {
    trait_bounds: BTreeMap<TypeVar, Vec<NamePath>>,
}

impl ConstraintPropagator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn propagate_to_caller(&mut self, caller_var: TypeVar, constraints: &[TypeConstraint], _origin: Option<String>) {
        for constraint in constraints {
            if let TypeConstraint::TraitBound { trait_name, .. } = constraint {
                self.trait_bounds.entry(caller_var.clone()).or_default().push(trait_name.clone());
            }
        }
    }

    pub fn compute_transitive_closure(&mut self) -> Result<(), ConstraintError> {
        Ok(())
    }

    pub fn get_trait_bounds(&self, var: &TypeVar) -> Option<&Vec<NamePath>> {
        self.trait_bounds.get(var)
    }
}

#[derive(Debug, Clone)]
struct RegisteredUnityVariant {
    name: Identifier,
    result_type: Option<HirType>,
}

#[derive(Debug, Clone)]
struct RegisteredUnity {
    base: Identifier,
    generics_len: usize,
    variants: Vec<RegisteredUnityVariant>,
}

#[derive(Debug, Default)]
pub struct ConstraintSolver {
    constraints: Vec<TypeConstraint>,
    substitutions: BTreeMap<TypeVar, HirType>,
    generic_bindings: BTreeMap<Identifier, HirType>,
    trait_bounds: BTreeMap<TypeVar, Vec<NamePath>>,
    trait_impls: Vec<TraitImpl>,
    multi_trait_bounds: Vec<MultiTraitBound>,
    where_clauses: Vec<WhereClause>,
    lifetime_constraints: Vec<LifetimeConstraint>,
    associated_type_constraints: Vec<AssociatedTypeConstraint>,
    associated_type_impls: Vec<(HirType, NamePath, Identifier, HirType)>,
    propagator: ConstraintPropagator,
    unity_types: Vec<RegisteredUnity>,
}

impl ConstraintSolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn constraints(&self) -> &[TypeConstraint] {
        &self.constraints
    }

    pub fn substitutions(&self) -> &BTreeMap<TypeVar, HirType> {
        &self.substitutions
    }

    pub fn generic_bindings(&self) -> &BTreeMap<Identifier, HirType> {
        &self.generic_bindings
    }

    pub fn trait_bounds(&self) -> &BTreeMap<TypeVar, Vec<NamePath>> {
        &self.trait_bounds
    }

    pub fn get_substitutions(&self) -> &BTreeMap<TypeVar, HirType> {
        &self.substitutions
    }

    pub fn add_constraint(&mut self, constraint: TypeConstraint) {
        if let TypeConstraint::TraitBound { var, trait_name, .. } = &constraint {
            self.trait_bounds.entry(var.clone()).or_default().push(trait_name.clone());
        }
        self.constraints.push(constraint);
    }

    pub fn add_substitution(&mut self, var: TypeVar, ty: HirType) {
        self.substitutions.insert(var, ty);
    }

    pub fn add_multi_trait_bound(&mut self, bound: MultiTraitBound) {
        self.multi_trait_bounds.push(bound);
    }

    pub fn add_where_clause(&mut self, clause: WhereClause) {
        self.where_clauses.push(clause);
    }

    pub fn add_lifetime_constraint(&mut self, constraint: LifetimeConstraint) {
        self.lifetime_constraints.push(constraint);
    }

    pub fn add_associated_type_constraint(&mut self, constraint: AssociatedTypeConstraint) {
        self.associated_type_constraints.push(constraint);
    }

    pub fn register_trait_impl(&mut self, item: TraitImpl) {
        self.trait_impls.push(item);
    }

    pub fn register_associated_type_impl(&mut self, base_type: HirType, trait_name: NamePath, assoc_name: Identifier, concrete_type: HirType) {
        self.associated_type_impls.push((base_type, trait_name, assoc_name, concrete_type));
    }

    pub fn resolve_associated_type(&self, base_type: &HirType, trait_name: &NamePath, assoc_name: &Identifier) -> Option<HirType> {
        self.associated_type_impls
            .iter()
            .find(|(base, trait_path, assoc, _)| base == base_type && trait_path == trait_name && assoc == assoc_name)
            .map(|(_, _, _, concrete)| concrete.clone())
    }

    pub fn register_unity_type(&mut self, enum_def: &HirEnum) {
        self.unity_types.push(RegisteredUnity {
            base: enum_def.name.clone(),
            generics_len: enum_def.generics.len(),
            variants: enum_def
                .variants
                .iter()
                .map(|variant| RegisteredUnityVariant { name: variant.name.clone(), result_type: variant.result_type.clone() })
                .collect(),
        });
    }

    fn apply_generic_bindings(&self, ty: &HirType) -> HirType {
        self.apply_generic_bindings_with_seen(ty, &mut BTreeSet::new())
    }

    fn apply_generic_bindings_with_seen(&self, ty: &HirType, seen: &mut BTreeSet<Identifier>) -> HirType {
        match ty {
            HirType::Generic(generic) => {
                let Some(bound) = self.generic_bindings.get(&generic.name)
                else {
                    return ty.clone();
                };
                if !seen.insert(generic.name.clone()) {
                    return ty.clone();
                }
                let resolved = self.apply_generic_bindings_with_seen(bound, seen);
                seen.remove(&generic.name);
                resolved
            }
            HirType::Apply(base, args) => HirType::Apply(
                Box::new(self.apply_generic_bindings_with_seen(base, seen)),
                args.iter().map(|arg| self.apply_generic_bindings_with_seen(arg, seen)).collect(),
            ),
            HirType::Tuple(items) => HirType::Tuple(items.iter().map(|item| self.apply_generic_bindings_with_seen(item, seen)).collect()),
            HirType::Array(item) => HirType::Array(Box::new(self.apply_generic_bindings_with_seen(item, seen))),
            HirType::Function(function) => HirType::Function(Box::new(FunctionType {
                params: function.params.iter().map(|param| self.apply_generic_bindings_with_seen(param, seen)).collect(),
                return_type: self.apply_generic_bindings_with_seen(&function.return_type, seen),
            })),
            HirType::Associated(associated) => HirType::Associated(Box::new(AssociatedType {
                base: self.apply_generic_bindings_with_seen(&associated.base, seen),
                name: associated.name.clone(),
                type_arguments: associated.type_arguments.iter().map(|arg| self.apply_generic_bindings_with_seen(arg, seen)).collect(),
            })),
            _ => ty.clone(),
        }
    }

    fn has_unresolved_generics(&self, ty: &HirType) -> bool {
        match ty {
            HirType::Generic(_) => true,
            HirType::Apply(base, args) => self.has_unresolved_generics(base) || args.iter().any(|arg| self.has_unresolved_generics(arg)),
            HirType::Tuple(items) => items.iter().any(|item| self.has_unresolved_generics(item)),
            HirType::Array(item) => self.has_unresolved_generics(item),
            HirType::Function(function) => {
                function.params.iter().any(|param| self.has_unresolved_generics(param)) || self.has_unresolved_generics(&function.return_type)
            }
            HirType::Associated(associated) => {
                self.has_unresolved_generics(&associated.base) || associated.type_arguments.iter().any(|arg| self.has_unresolved_generics(arg))
            }
            _ => false,
        }
    }

    fn bind_generic(&mut self, name: &Identifier, actual: &HirType, span: Option<SourceSpan>) -> Result<(), ConstraintError> {
        let actual = self.apply_generic_bindings(actual);
        if matches!(&actual, HirType::Generic(generic) if &generic.name == name) {
            return Ok(());
        }
        if let Some(bound) = self.generic_bindings.get(name).cloned() {
            if let HirType::Generic(actual_generic) = &actual {
                if !self.generic_bindings.contains_key(&actual_generic.name) {
                    self.generic_bindings.insert(actual_generic.name.clone(), bound);
                    return Ok(());
                }
            }
            return self.unify(&self.apply_generic_bindings(&bound), &actual, span);
        }
        if let HirType::Generic(actual_generic) = &actual {
            if let Some(bound) = self.generic_bindings.get(&actual_generic.name).cloned() {
                self.generic_bindings.insert(name.clone(), bound);
                return Ok(());
            }
        }
        self.generic_bindings.insert(name.clone(), actual);
        Ok(())
    }

    fn collect_generic_bindings(&mut self, left: &HirType, right: &HirType, span: Option<SourceSpan>) -> Result<(), ConstraintError> {
        let left = self.apply_generic_bindings(left);
        let right = self.apply_generic_bindings(right);
        match (&left, &right) {
            (HirType::Generic(generic), actual) => self.bind_generic(&generic.name, actual, span),
            (actual, HirType::Generic(generic)) => self.bind_generic(&generic.name, actual, span),
            (HirType::Apply(lhs_base, lhs_args), HirType::Apply(rhs_base, rhs_args)) => {
                self.collect_generic_bindings(lhs_base, rhs_base, span.clone())?;
                for (lhs, rhs) in lhs_args.iter().zip(rhs_args) {
                    self.collect_generic_bindings(lhs, rhs, span.clone())?;
                }
                Ok(())
            }
            (HirType::Tuple(lhs_items), HirType::Tuple(rhs_items)) => {
                for (lhs, rhs) in lhs_items.iter().zip(rhs_items) {
                    self.collect_generic_bindings(lhs, rhs, span.clone())?;
                }
                Ok(())
            }
            (HirType::Array(lhs_item), HirType::Array(rhs_item)) => self.collect_generic_bindings(lhs_item, rhs_item, span),
            (HirType::Function(lhs_fn), HirType::Function(rhs_fn)) => {
                let lhs_params = &lhs_fn.params;
                let rhs_params = &rhs_fn.params;
                for (lhs, rhs) in lhs_params.iter().zip(rhs_params) {
                    self.collect_generic_bindings(lhs, rhs, span.clone())?;
                }
                self.collect_generic_bindings(&lhs_fn.return_type, &rhs_fn.return_type, span)
            }
            (HirType::Associated(lhs_assoc), HirType::Associated(rhs_assoc))
                if lhs_assoc.name == rhs_assoc.name && lhs_assoc.type_arguments.len() == rhs_assoc.type_arguments.len() =>
            {
                self.collect_generic_bindings(&lhs_assoc.base, &rhs_assoc.base, span.clone())?;
                for (lhs, rhs) in lhs_assoc.type_arguments.iter().zip(&rhs_assoc.type_arguments) {
                    self.collect_generic_bindings(lhs, rhs, span.clone())?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn solve_where_clauses(&self) -> Result<(), ConstraintError> {
        for clause in &self.where_clauses {
            for bound in &clause.bounds {
                let resolved_ty = self.apply_generic_bindings(&bound.ty);
                if self.has_unresolved_generics(&resolved_ty) {
                    continue;
                }
                for trait_name in &bound.traits {
                    if !self.check_trait_bound(&resolved_ty, trait_name)? {
                        return Err(ConstraintError::trait_not_implemented(
                            &format!("{:?}", resolved_ty),
                            &display_path(trait_name),
                            bound.span.clone().or_else(|| clause.span.clone()),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn unify(&mut self, left: &HirType, right: &HirType, span: Option<SourceSpan>) -> Result<(), ConstraintError> {
        if left == &HirType::AutoType || right == &HirType::AutoType {
            return Ok(());
        }
        match (left, right) {
            (HirType::Array(lhs), HirType::Array(rhs)) => self.unify(lhs, rhs, span),
            (HirType::Tuple(lhs), HirType::Tuple(rhs)) => {
                if lhs.len() != rhs.len() {
                    return Err(ConstraintError::type_mismatch(left.clone(), right.clone(), span));
                }
                for (lhs, rhs) in lhs.iter().zip(rhs) {
                    self.unify(lhs, rhs, span.clone())?;
                }
                Ok(())
            }
            (HirType::Function(lhs_fn), HirType::Function(rhs_fn)) => {
                let lhs_params = &lhs_fn.params;
                let rhs_params = &rhs_fn.params;
                if lhs_params.len() != rhs_params.len() {
                    return Err(ConstraintError::type_mismatch(left.clone(), right.clone(), span));
                }
                for (lhs, rhs) in lhs_params.iter().zip(rhs_params) {
                    self.unify(lhs, rhs, span.clone())?;
                }
                self.unify(&lhs_fn.return_type, &rhs_fn.return_type, span)
            }
            (HirType::Associated(lhs_assoc), HirType::Associated(rhs_assoc)) => {
                if lhs_assoc.name != rhs_assoc.name || lhs_assoc.type_arguments.len() != rhs_assoc.type_arguments.len() {
                    return Err(ConstraintError::type_mismatch(left.clone(), right.clone(), span));
                }
                self.unify(&lhs_assoc.base, &rhs_assoc.base, span.clone())?;
                for (lhs, rhs) in lhs_assoc.type_arguments.iter().zip(&rhs_assoc.type_arguments) {
                    self.unify(lhs, rhs, span.clone())?;
                }
                Ok(())
            }
            _ if left == right => Ok(()),
            _ => Err(ConstraintError::type_mismatch(left.clone(), right.clone(), span)),
        }
    }

    pub fn check_trait_bound(&self, ty: &HirType, trait_name: &NamePath) -> Result<bool, ConstraintError> {
        if matches!(display_path(trait_name).as_str(), "Copy" | "Clone" | "Eq" | "Send" | "Sync" | "Default")
            && matches!(
                ty,
                HirType::Integer32 { signed: _ } | HirType::Integer64 { signed: _ } | HirType::Float32 | HirType::Float64 | HirType::Boolean
            )
        {
            return Ok(true);
        }
        Ok(self.trait_impls.iter().any(|item| &item.ty == ty && item.trait_name == *trait_name))
    }

    pub fn is_subtype(&self, sub: &HirType, sup: &HirType) -> bool {
        if sub == sup {
            return true;
        }
        if matches!((sub, sup), (HirType::Integer32 { signed: _ }, HirType::Integer64 { signed: _ }) | (HirType::Float32, HirType::Float64)) {
            return true;
        }
        self.is_unity_variant_subtype(sub, sup)
    }

    fn is_unity_variant_subtype(&self, sub: &HirType, sup: &HirType) -> bool {
        let HirType::Named(variant_name) = sub
        else {
            return false;
        };

        for unity in &self.unity_types {
            for variant in &unity.variants {
                if &variant.name != variant_name {
                    continue;
                }
                if let Some(result_type) = &variant.result_type {
                    return self.matches_type_pattern(result_type, sup, &mut BTreeMap::new());
                }
                return match sup {
                    HirType::Named(base) => unity.generics_len == 0 && base == &unity.base,
                    HirType::Apply(base, args) => {
                        matches!(base.as_ref(), HirType::Named(base_name) if base_name == &unity.base) && args.len() == unity.generics_len
                    }
                    _ => false,
                };
            }
        }

        false
    }

    fn matches_type_pattern(&self, pattern: &HirType, concrete: &HirType, bindings: &mut BTreeMap<Identifier, HirType>) -> bool {
        match (pattern, concrete) {
            (HirType::Generic(generic), actual) => match bindings.get(&generic.name) {
                Some(bound) => bound == actual,
                None => {
                    bindings.insert(generic.name.clone(), actual.clone());
                    true
                }
            },
            (HirType::Named(lhs), HirType::Named(rhs)) => lhs == rhs,
            (HirType::Apply(lhs_base, lhs_args), HirType::Apply(rhs_base, rhs_args)) => {
                lhs_args.len() == rhs_args.len()
                    && self.matches_type_pattern(lhs_base, rhs_base, bindings)
                    && lhs_args.iter().zip(rhs_args).all(|(lhs, rhs)| self.matches_type_pattern(lhs, rhs, bindings))
            }
            (HirType::Tuple(lhs_items), HirType::Tuple(rhs_items)) => {
                lhs_items.len() == rhs_items.len()
                    && lhs_items.iter().zip(rhs_items).all(|(lhs, rhs)| self.matches_type_pattern(lhs, rhs, bindings))
            }
            (HirType::Array(lhs_item), HirType::Array(rhs_item)) => self.matches_type_pattern(lhs_item, rhs_item, bindings),
            (HirType::Function(lhs_fn), HirType::Function(rhs_fn)) => {
                lhs_fn.params.len() == rhs_fn.params.len()
                    && lhs_fn.params.iter().zip(&rhs_fn.params).all(|(lhs, rhs)| self.matches_type_pattern(lhs, rhs, bindings))
                    && self.matches_type_pattern(&lhs_fn.return_type, &rhs_fn.return_type, bindings)
            }
            (HirType::Associated(lhs_assoc), HirType::Associated(rhs_assoc)) => {
                lhs_assoc.name == rhs_assoc.name
                    && lhs_assoc.type_arguments.len() == rhs_assoc.type_arguments.len()
                    && self.matches_type_pattern(&lhs_assoc.base, &rhs_assoc.base, bindings)
                    && lhs_assoc
                        .type_arguments
                        .iter()
                        .zip(&rhs_assoc.type_arguments)
                        .all(|(lhs, rhs)| self.matches_type_pattern(lhs, rhs, bindings))
            }
            _ => pattern == concrete,
        }
    }

    pub fn solve(&mut self) -> Result<(), ConstraintError> {
        for constraint in self.constraints.clone() {
            match constraint {
                TypeConstraint::TraitBound { var, trait_name, span } => {
                    if let Some(ty) = self.substitutions.get(&var) {
                        let ty = self.apply_generic_bindings(ty);
                        if !self.check_trait_bound(&ty, &trait_name)? {
                            return Err(ConstraintError::trait_not_implemented(&format!("{:?}", ty), &display_path(&trait_name), span));
                        }
                    }
                }
                TypeConstraint::Equality { left, right, span } => {
                    self.collect_generic_bindings(&left, &right, span.clone())?;
                    let left = self.apply_generic_bindings(&left);
                    let right = self.apply_generic_bindings(&right);
                    self.unify(&left, &right, span)?;
                }
                TypeConstraint::Subtype { sub, sup, span } => {
                    self.collect_generic_bindings(&sub, &sup, span.clone())?;
                    let sub = self.apply_generic_bindings(&sub);
                    let sup = self.apply_generic_bindings(&sup);
                    if !self.is_subtype(&sub, &sup) {
                        return Err(ConstraintError::type_mismatch(sup, sub, span));
                    }
                }
            }
        }

        for constraint in &self.lifetime_constraints {
            if let LifetimeConstraintKind::Outlives { longer, shorter } = &constraint.kind {
                if shorter == &Lifetime::static_lifetime() && longer != &Lifetime::static_lifetime() {
                    return Err(ConstraintError::propagation_failed("lifetime does not outlive 'static", constraint.span.clone()));
                }
            }
        }

        for constraint in self.associated_type_constraints.clone() {
            let base_type = self.apply_generic_bindings(&constraint.base_type);
            let expected = self.apply_generic_bindings(&constraint.expected);
            let Some(actual) = self.resolve_associated_type(&base_type, &constraint.trait_name, &constraint.assoc_name)
            else {
                return Err(ConstraintError::associated_type_not_found(
                    base_type,
                    constraint.trait_name.clone(),
                    constraint.assoc_name.clone(),
                    constraint.span,
                ));
            };
            let actual = self.apply_generic_bindings(&actual);
            self.collect_generic_bindings(&expected, &actual, constraint.span.clone())?;
            let expected = self.apply_generic_bindings(&expected);
            self.unify(&actual, &expected, constraint.span)?;
        }

        self.solve_where_clauses()?;

        Ok(())
    }

    pub fn propagate_to_caller(&mut self, caller_var: TypeVar, constraints: &[TypeConstraint], origin: Option<String>) {
        self.propagator.propagate_to_caller(caller_var.clone(), constraints, origin);
        if let Some(bounds) = self.propagator.get_trait_bounds(&caller_var) {
            self.trait_bounds.insert(caller_var, bounds.clone());
        }
    }

    pub fn suggest_fixes(&self) -> Vec<FixSuggestion> {
        if !self.constraints.is_empty() || !self.trait_bounds.is_empty() {
            vec![FixSuggestion::new("补充 trait 实现或调整类型约束", 1)]
        }
        else {
            vec![]
        }
    }

    pub fn generate_error_report(&self, error: &ConstraintError) -> ConstraintReport {
        let mut report = ConstraintReport::new(error.kind.clone(), error.span.clone(), error.message.clone());
        report.add_suggestion(FixSuggestion::new("检查相关类型与约束", 1));
        if let ConstraintErrorKind::TypeMismatch { expected, found } = &error.kind {
            report.related_types.push(format!("{:?}", expected));
            report.related_types.push(format!("{:?}", found));
        }
        report
    }

    pub fn clear(&mut self) {
        self.constraints.clear();
        self.substitutions.clear();
        self.generic_bindings.clear();
        self.trait_bounds.clear();
        self.trait_impls.clear();
        self.multi_trait_bounds.clear();
        self.where_clauses.clear();
        self.lifetime_constraints.clear();
        self.associated_type_constraints.clear();
        self.associated_type_impls.clear();
        self.unity_types.clear();
    }
}
