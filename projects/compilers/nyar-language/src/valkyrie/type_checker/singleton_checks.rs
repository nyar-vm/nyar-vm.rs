use std::collections::{BTreeMap, BTreeSet};

use crate::types::{
    Identifier, SourceSpan,
    hir::{HirBlock, HirExpr, HirExprKind, HirFunction, HirModule, HirSingleton, HirStatement, HirStatementKind, ValkyrieType},
};

use super::last_name;

/// Singleton constructor method name recognized by semantic analysis.
const CONSTRUCTOR_NAME: &str = "init";
/// Singleton finalizer method name recognized by semantic analysis.
const FINALIZER_NAME: &str = "finalize";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SingletonErrorKind {
    ConstructorForbidden { singleton: Identifier },
    ReadonlyFieldWrite { singleton: Identifier, field: Identifier },
    GenericsForbidden { singleton: Identifier },
    DuplicateLifecycleMethod { singleton: Identifier, method: Identifier },
    ConstructorSignatureInvalid { singleton: Identifier },
    FinalizerSignatureInvalid { singleton: Identifier },
    FinalizerOnEagerSingleton { singleton: Identifier },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SingletonError {
    pub kind: SingletonErrorKind,
    pub message: String,
    pub span: Option<SourceSpan>,
}

impl SingletonError {
    pub fn constructor_forbidden(singleton: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: SingletonErrorKind::ConstructorForbidden { singleton: singleton.clone() },
            message: format!("singleton `{singleton}` cannot be constructed"),
            span,
        }
    }

    pub fn readonly_field_write(singleton: Identifier, field: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: SingletonErrorKind::ReadonlyFieldWrite { singleton: singleton.clone(), field: field.clone() },
            message: format!("readonly field write denied: {singleton}.{field}"),
            span,
        }
    }

    pub fn generics_forbidden(singleton: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: SingletonErrorKind::GenericsForbidden { singleton: singleton.clone() },
            message: format!(
                "generic singleton `{singleton}` is not allowed: singleton semantics require exactly one global instance and cannot be parameterized by type arguments"
            ),
            span,
        }
    }

    pub fn duplicate_lifecycle_method(singleton: Identifier, method: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: SingletonErrorKind::DuplicateLifecycleMethod { singleton: singleton.clone(), method: method.clone() },
            message: format!("singleton `{singleton}` defines multiple `{method}` methods; at most one is allowed"),
            span,
        }
    }

    pub fn constructor_signature_invalid(singleton: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: SingletonErrorKind::ConstructorSignatureInvalid { singleton: singleton.clone() },
            message: format!("singleton `{singleton}` constructor `init` must take exactly one `self` parameter and return unit"),
            span,
        }
    }

    pub fn finalizer_signature_invalid(singleton: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: SingletonErrorKind::FinalizerSignatureInvalid { singleton: singleton.clone() },
            message: format!("singleton `{singleton}` finalizer `finalize` must take exactly one `self` parameter and return unit"),
            span,
        }
    }

    pub fn finalizer_on_eager_singleton(singleton: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: SingletonErrorKind::FinalizerOnEagerSingleton { singleton: singleton.clone() },
            message: format!("singleton `{singleton}` is eager and cannot define a finalizer; only lazy singletons support unload"),
            span,
        }
    }
}

impl std::fmt::Display for SingletonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SingletonError {}

#[derive(Debug, Default)]
pub struct SingletonChecker {
    singletons: BTreeMap<Identifier, HirSingleton>,
    singleton_names: BTreeSet<Identifier>,
    mutable_fields: BTreeSet<(Identifier, Identifier)>,
    errors: Vec<SingletonError>,
}

impl SingletonChecker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check_module(&mut self, module: &HirModule) -> Vec<SingletonError> {
        self.errors.clear();
        self.singletons.clear();
        self.singleton_names.clear();
        self.mutable_fields.clear();

        for singleton in &module.singletons {
            self.singleton_names.insert(singleton.name.clone());
            self.singletons.insert(singleton.name.clone(), singleton.clone());
            for field in &singleton.fields {
                if field.is_mutable {
                    self.mutable_fields.insert((singleton.name.clone(), field.name.clone()));
                }
            }
            self.check_generics_forbidden(singleton);
            self.check_lifecycle_methods(singleton);
        }

        for function in &module.functions {
            let mut env = type_env_from_params(function);
            self.walk_block(&function.body, &mut env);
        }
        for singleton in &module.singletons {
            for method in &singleton.methods {
                let mut env = type_env_from_params(method);
                env.insert(Identifier::new("self"), ValkyrieType::Named(singleton.name.clone()));
                self.walk_block(&method.body, &mut env);
            }
            if let Some(constructor) = &singleton.constructor {
                let mut env = type_env_from_params(constructor);
                env.insert(Identifier::new("self"), ValkyrieType::Named(singleton.name.clone()));
                self.walk_block(&constructor.body, &mut env);
            }
            if let Some(finalizer) = &singleton.finalizer {
                let mut env = type_env_from_params(finalizer);
                env.insert(Identifier::new("self"), ValkyrieType::Named(singleton.name.clone()));
                self.walk_block(&finalizer.body, &mut env);
            }
        }

        self.errors.clone()
    }

    fn check_generics_forbidden(&mut self, singleton: &HirSingleton) {
        if !singleton.generics.is_empty() {
            self.errors.push(SingletonError::generics_forbidden(singleton.name.clone(), None));
        }
    }

    fn check_lifecycle_methods(&mut self, singleton: &HirSingleton) {
        let mut constructor_count = 0usize;
        let mut finalizer_count = 0usize;
        for method in &singleton.methods {
            if method.name.as_str() == CONSTRUCTOR_NAME {
                constructor_count += 1;
            }
            if method.name.as_str() == FINALIZER_NAME {
                finalizer_count += 1;
            }
        }
        if constructor_count > 0 {
            self.errors.push(SingletonError::duplicate_lifecycle_method(singleton.name.clone(), Identifier::new(CONSTRUCTOR_NAME), None));
        }
        if finalizer_count > 0 {
            self.errors.push(SingletonError::duplicate_lifecycle_method(singleton.name.clone(), Identifier::new(FINALIZER_NAME), None));
        }
        if let Some(constructor) = &singleton.constructor {
            if !is_lifecycle_signature_valid(constructor) {
                self.errors.push(SingletonError::constructor_signature_invalid(singleton.name.clone(), None));
            }
        }
        if let Some(finalizer) = &singleton.finalizer {
            if !is_lifecycle_signature_valid(finalizer) {
                self.errors.push(SingletonError::finalizer_signature_invalid(singleton.name.clone(), None));
            }
            if !singleton.is_lazy {
                self.errors.push(SingletonError::finalizer_on_eager_singleton(singleton.name.clone(), None));
            }
        }
    }

    pub fn errors(&self) -> &[SingletonError] {
        &self.errors
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
                    if let crate::types::hir::HirPattern::Variable(name) = pattern {
                        env.insert(name.name.clone(), ty.clone());
                    }
                }
            }
            HirStatementKind::Expr(expr) => self.walk_expr(expr, env),
        }
    }

    fn walk_expr(&mut self, expr: &HirExpr, env: &mut BTreeMap<Identifier, ValkyrieType>) {
        match &expr.kind {
            HirExprKind::Construct { name, args, .. } => {
                if self.singleton_names.contains(name) {
                    self.errors.push(SingletonError::constructor_forbidden(name.clone(), Some(expr.span.clone())));
                }
                for arg in args {
                    self.walk_expr(arg, env);
                }
            }
            HirExprKind::StoreField { object, field, value, .. } => {
                self.walk_expr(object, env);
                self.walk_expr(value, env);
                if let Some(owner) = resolve_singleton_owner(object, env, &self.singleton_names) {
                    if !self.mutable_fields.contains(&(owner.clone(), field.clone())) {
                        self.errors.push(SingletonError::readonly_field_write(owner, field.clone(), Some(expr.span.clone())));
                    }
                }
            }
            HirExprKind::FieldAccess { object, .. } => self.walk_expr(object, env),
            HirExprKind::Call { callee, args, .. } => {
                self.walk_expr(callee, env);
                for arg in crate::types::hir::hir_call_arg_values(args) {
                    self.walk_expr(arg, env);
                }
            }
            HirExprKind::If { condition, then_branch, else_branch } => {
                self.walk_expr(condition, env);
                self.walk_block(then_branch, env);
                if let Some(else_branch) = else_branch {
                    self.walk_block(else_branch, env);
                }
            }
            HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
                self.walk_expr(scrutinee, env);
                for arm in arms {
                    self.walk_expr(&arm.body, env);
                    if let Some(guard) = &arm.guard {
                        self.walk_expr(guard, env);
                    }
                }
            }
            HirExprKind::Block(block) => self.walk_block(block, env),
            _ => {}
        }
    }
}

/// Checks that a lifecycle method (constructor `init` or finalizer `finalize`)
/// has the canonical signature: exactly one `self` parameter and `unit` / `void` return type.
fn is_lifecycle_signature_valid(function: &HirFunction) -> bool {
    if function.params.len() != 1 {
        return false;
    }
    let param = &function.params[0];
    if param.name.name.as_str() != "self" {
        return false;
    }
    matches!(function.return_type, ValkyrieType::Unit | ValkyrieType::Void)
}

fn type_env_from_params(function: &HirFunction) -> BTreeMap<Identifier, ValkyrieType> {
    function.params.iter().map(|param| (param.name.name.clone(), param.ty.clone())).collect()
}

fn resolve_singleton_owner(
    object: &HirExpr,
    env: &BTreeMap<Identifier, ValkyrieType>,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<Identifier> {
    match &object.kind {
        HirExprKind::Variable(identifier) if singleton_names.contains(&identifier.name) => Some(identifier.name.clone()),
        HirExprKind::Path(path) if path.parts().len() == 1 && singleton_names.contains(&path.parts()[0]) => Some(path.parts()[0].clone()),
        HirExprKind::Variable(identifier) => match env.get(&identifier.name)? {
            ValkyrieType::Named(name) if singleton_names.contains(name) => Some(name.clone()),
            _ => None,
        },
        HirExprKind::Path(path) => last_name(path).filter(|name| singleton_names.contains(name)),
        _ => None,
    }
}
