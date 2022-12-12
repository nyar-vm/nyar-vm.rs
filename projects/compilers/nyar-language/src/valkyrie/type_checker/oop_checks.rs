use std::collections::{BTreeMap, BTreeSet};

use crate::types::{
    Identifier, NamePath,
    hir::{
        AccessLevel, HirBlock, HirExpr, HirExprKind, HirExtractorPattern, HirFunction, HirMatchArm, HirModule, HirPattern, HirStatement,
        HirStatementKind, HirStruct, HirVisibility, ValkyrieType,
    },
};

use super::{AccessContext, AccessControlError, EnumRegistry, ExhaustivenessChecker, SealedClassError, SealedClassRegistry, last_name};

/// Builds a sealed-class registry from HIR structs (base + direct subclasses / ADT variants).
pub fn fill_sealed_class_registry(module: &HirModule) -> SealedClassRegistry {
    let mut registry = SealedClassRegistry::new();
    for class in &module.structs {
        registry.register_sealed_class(class);
    }
    let sealed_names = module.structs.iter().filter(|class| class.is_sealed).map(|class| class.name.clone()).collect::<BTreeSet<_>>();
    for class in &module.structs {
        for parent in &class.parents {
            if let Some(parent_name) = last_name(&parent.name) {
                if sealed_names.contains(&parent_name) {
                    let _ = registry.register_subclass(&parent_name, &class.name);
                }
            }
        }
    }
    registry
}

#[derive(Debug, Default)]
pub struct SealedMatchChecker {
    errors: Vec<SealedClassError>,
}

impl SealedMatchChecker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check_module(&mut self, module: &HirModule) -> Vec<SealedClassError> {
        self.errors.clear();
        let registry = fill_sealed_class_registry(module);
        let enum_registry = EnumRegistry::from_module(module);
        let checker = ExhaustivenessChecker::with_enum_registry(registry.clone(), enum_registry);
        let struct_fields = struct_field_types(module);

        for function in &module.functions {
            let mut env = type_env_from_params(function);
            self.walk_block(&function.body, &mut env, &registry, &checker, &struct_fields);
        }
        for class in &module.structs {
            for method in &class.methods {
                let mut env = type_env_from_params(method);
                env.insert(Identifier::new("self"), ValkyrieType::Named(class.name.clone()));
                self.walk_block(&method.body, &mut env, &registry, &checker, &struct_fields);
            }
        }
        for singleton in &module.singletons {
            for method in &singleton.methods {
                let mut env = type_env_from_params(method);
                env.insert(Identifier::new("self"), ValkyrieType::Named(singleton.name.clone()));
                self.walk_block(&method.body, &mut env, &registry, &checker, &struct_fields);
            }
        }
        self.errors.clone()
    }

    pub fn errors(&self) -> &[SealedClassError] {
        &self.errors
    }

    fn walk_block(
        &mut self,
        block: &HirBlock,
        env: &mut BTreeMap<Identifier, ValkyrieType>,
        registry: &SealedClassRegistry,
        checker: &ExhaustivenessChecker,
        struct_fields: &BTreeMap<Identifier, BTreeMap<Identifier, ValkyrieType>>,
    ) {
        for statement in &block.statements {
            self.walk_statement(statement, env, registry, checker, struct_fields);
        }
        if let Some(expr) = &block.expr {
            self.walk_expr(expr, env, registry, checker, struct_fields);
        }
    }

    fn walk_statement(
        &mut self,
        statement: &HirStatement,
        env: &mut BTreeMap<Identifier, ValkyrieType>,
        registry: &SealedClassRegistry,
        checker: &ExhaustivenessChecker,
        struct_fields: &BTreeMap<Identifier, BTreeMap<Identifier, ValkyrieType>>,
    ) {
        match &statement.kind {
            HirStatementKind::Let { pattern, initializer, ty, .. } => {
                if let Some(value) = initializer {
                    self.walk_expr(value, env, registry, checker, struct_fields);
                }
                if let Some(ty) = ty {
                    if let HirPattern::Variable(name) = pattern {
                        env.insert(name.name.clone(), ty.clone());
                    }
                }
            }
            HirStatementKind::Expr(expr) => self.walk_expr(expr, env, registry, checker, struct_fields),
        }
    }

    fn walk_expr(
        &mut self,
        expr: &HirExpr,
        env: &mut BTreeMap<Identifier, ValkyrieType>,
        registry: &SealedClassRegistry,
        checker: &ExhaustivenessChecker,
        struct_fields: &BTreeMap<Identifier, BTreeMap<Identifier, ValkyrieType>>,
    ) {
        match &expr.kind {
            HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
                self.walk_expr(scrutinee, env, registry, checker, struct_fields);
                for arm in arms {
                    self.walk_expr(&arm.body, env, registry, checker, struct_fields);
                    if let Some(guard) = &arm.guard {
                        self.walk_expr(guard, env, registry, checker, struct_fields);
                    }
                }
                self.check_match(scrutinee, arms, env, registry, checker, struct_fields);
            }
            HirExprKind::Call { callee, args, .. } => {
                self.walk_expr(callee, env, registry, checker, struct_fields);
                for arg in crate::types::hir::hir_call_arg_values(args) {
                    self.walk_expr(arg, env, registry, checker, struct_fields);
                }
            }
            HirExprKind::FieldAccess { object, .. } => self.walk_expr(object, env, registry, checker, struct_fields),
            HirExprKind::StoreField { object, value, .. } => {
                self.walk_expr(object, env, registry, checker, struct_fields);
                self.walk_expr(value, env, registry, checker, struct_fields);
            }
            HirExprKind::If { condition, then_branch, else_branch }
            | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
                self.walk_expr(condition, env, registry, checker, struct_fields);
                self.walk_block(then_branch, env, registry, checker, struct_fields);
                if let Some(else_branch) = else_branch {
                    self.walk_block(else_branch, env, registry, checker, struct_fields);
                }
            }
            HirExprKind::Block(block) => self.walk_block(block, env, registry, checker, struct_fields),
            HirExprKind::Lambda { body, params, .. } => {
                let mut nested = env.clone();
                for param in params {
                    nested.insert(param.name.name.clone(), param.ty.clone());
                }
                self.walk_block(body, &mut nested, registry, checker, struct_fields);
            }
            _ => {}
        }
    }

    fn check_match(
        &mut self,
        scrutinee: &HirExpr,
        arms: &[HirMatchArm],
        env: &BTreeMap<Identifier, ValkyrieType>,
        registry: &SealedClassRegistry,
        checker: &ExhaustivenessChecker,
        struct_fields: &BTreeMap<Identifier, BTreeMap<Identifier, ValkyrieType>>,
    ) {
        // A wildcard/variable/else arm only counts as full coverage when it is unguarded:
        // a guarded wildcard (`case _ if cond:`) may still fail the guard and fall through.
        let has_wildcard = arms
            .iter()
            .any(|arm| arm.guard.is_none() && matches!(arm.pattern, HirPattern::Wildcard | HirPattern::Else | HirPattern::Variable(_)));
        if checker.is_wildcard_exhaustive(&Identifier::new("_"), has_wildcard) {
            return;
        }

        // Split covered variants into unconditionally covered (no guard) and conditionally
        // covered (guarded). A guarded arm does not fully cover its variant because the guard
        // may fail at runtime, so exhaustiveness only considers `unconditional_covered`.
        let mut unconditional_covered: Vec<Identifier> = Vec::new();
        let mut conditional_covered: Vec<Identifier> = Vec::new();
        for arm in arms {
            let variants = covered_variants(&arm.pattern);
            if arm.guard.is_some() {
                conditional_covered.extend(variants);
            }
            else {
                unconditional_covered.extend(variants);
            }
        }

        // Duplicate arm detection runs over every covered variant, including guarded arms and
        // each sub-pattern of an `Or` pattern (flattened by `covered_variants`).
        let mut all_covered = unconditional_covered.clone();
        all_covered.extend(conditional_covered.iter().cloned());
        if let Some(error) = checker.check_duplicate_arms(&all_covered) {
            self.errors.push(error);
        }

        // The sealed/enum name is inferred from all covered variants so that a match consisting
        // solely of guarded arms is still attributed to the right sealed class / sum type.
        let sealed_name =
            infer_sealed_scrutinee(scrutinee, env, registry, struct_fields).or_else(|| sealed_from_covered_variants(registry, &all_covered));
        if let Some(sealed_name) = sealed_name {
            if registry.is_sealed_class(&sealed_name) {
                if let Err(error) = checker.check_exhaustiveness(&sealed_name, &unconditional_covered) {
                    self.errors.push(error);
                }
            }
            return;
        }

        let enum_name = infer_sum_type_scrutinee(scrutinee, env, checker.enum_registry(), struct_fields)
            .or_else(|| checker.enum_registry().enum_from_covered_variants(&all_covered));
        let Some(enum_name) = enum_name
        else {
            return;
        };
        if !checker.enum_registry().is_sum_type(&enum_name) {
            return;
        }
        if let Err(error) = checker.check_variant_exhaustiveness(&enum_name, &unconditional_covered) {
            self.errors.push(error);
        }
    }
}

fn type_env_from_params(function: &HirFunction) -> BTreeMap<Identifier, ValkyrieType> {
    function.params.iter().map(|param| (param.name.name.clone(), param.ty.clone())).collect()
}

fn struct_field_types(module: &HirModule) -> BTreeMap<Identifier, BTreeMap<Identifier, ValkyrieType>> {
    let mut map = BTreeMap::new();
    // Local structs plus dependency-exported aggregates: `match expr.kind` in a
    // consumer module must resolve FieldAccess when `expr: HirExpr` is imported.
    let structs = module.structs.iter().chain(module.imported_semantic_exports.iter().flat_map(|export| export.structs.iter()));
    for class in structs {
        let fields = class.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect();
        map.insert(class.name.clone(), fields);
    }
    for singleton in &module.singletons {
        let fields = singleton.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect();
        map.insert(singleton.name.clone(), fields);
    }
    map
}

fn infer_sealed_scrutinee(
    scrutinee: &HirExpr,
    env: &BTreeMap<Identifier, ValkyrieType>,
    registry: &SealedClassRegistry,
    struct_fields: &BTreeMap<Identifier, BTreeMap<Identifier, ValkyrieType>>,
) -> Option<Identifier> {
    let type_name = infer_scrutinee_nominal_type(scrutinee, env, struct_fields)?;
    if registry.is_sealed_class(&type_name) { Some(type_name) } else { None }
}

fn infer_sum_type_scrutinee(
    scrutinee: &HirExpr,
    env: &BTreeMap<Identifier, ValkyrieType>,
    enum_registry: &EnumRegistry,
    struct_fields: &BTreeMap<Identifier, BTreeMap<Identifier, ValkyrieType>>,
) -> Option<Identifier> {
    let type_name = infer_scrutinee_nominal_type(scrutinee, env, struct_fields)?;
    if enum_registry.is_sum_type(&type_name) { Some(type_name) } else { None }
}

/// Resolve the nominal type of a match scrutinee, including `expr.kind` FieldAccess.
fn infer_scrutinee_nominal_type(
    scrutinee: &HirExpr,
    env: &BTreeMap<Identifier, ValkyrieType>,
    struct_fields: &BTreeMap<Identifier, BTreeMap<Identifier, ValkyrieType>>,
) -> Option<Identifier> {
    match &scrutinee.kind {
        HirExprKind::Variable(identifier) => nominal_type_name(env.get(&identifier.name)?),
        HirExprKind::Path(path) => {
            let name = last_name(path)?;
            nominal_type_name(env.get(&name)?)
        }
        HirExprKind::FieldAccess { object, field } => {
            let owner_type = infer_scrutinee_nominal_type(object, env, struct_fields)?;
            let field_ty = struct_fields.get(&owner_type)?.get(field)?;
            nominal_type_name(field_ty)
        }
        _ => None,
    }
}

/// `Named(T)` 或 `Apply(Named(T), …)` → `T`。
fn nominal_type_name(ty: &ValkyrieType) -> Option<Identifier> {
    match ty {
        ValkyrieType::Named(name) => Some(name.clone()),
        ValkyrieType::Apply(base, _) => match base.as_ref() {
            ValkyrieType::Named(name) => Some(name.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn sealed_from_covered_variants(registry: &SealedClassRegistry, covered: &[Identifier]) -> Option<Identifier> {
    let mut candidates = BTreeSet::new();
    for class in registry.sealed_class_names() {
        let permitted = registry.get_permitted_subclasses(class);
        if covered.iter().any(|variant| permitted.contains(variant)) {
            candidates.insert(class.clone());
        }
    }
    if candidates.len() == 1 { candidates.into_iter().next() } else { None }
}

fn covered_variants(pattern: &HirPattern) -> Vec<Identifier> {
    match pattern {
        HirPattern::Extractor(HirExtractorPattern::Constructor { name, canonical_callee, .. }) => {
            // Unite arms are lowered as `Variant.extractor`. Exhaustiveness must
            // credit the variant head (`IntegerLiteral` / `Fine`), not the trailing
            // `extractor` method name — otherwise HirExprKind/Result matches look
            // non-exhaustive even when every arm is present.
            if let Some(variant) = variant_head_before_extractor(canonical_callee) {
                return vec![variant];
            }
            if let Some(variant) = variant_head_before_extractor(name) {
                return vec![variant];
            }
            last_name(name).or_else(|| last_name(canonical_callee)).into_iter().collect()
        }
        HirPattern::Extractor(HirExtractorPattern::Array { canonical_callee, .. }) => last_name(canonical_callee).into_iter().collect(),
        HirPattern::Name(name) | HirPattern::Type(name) => {
            // Name→Extractor transitional paths may still carry `Variant.extractor`.
            if let Some(variant) = variant_head_before_extractor(name) {
                return vec![variant];
            }
            last_name(name).into_iter().collect()
        }
        HirPattern::Object { name: Some(name), .. } => {
            // Unite destructuring `case IntegerLiteral { text }:` lowers as Object.
            if let Some(variant) = variant_head_before_extractor(name) {
                return vec![variant];
            }
            last_name(name).into_iter().collect()
        }
        HirPattern::Or(patterns) => patterns.iter().flat_map(covered_variants).collect(),
        HirPattern::Bind { pattern, .. } | HirPattern::Mut(pattern) | HirPattern::Pin { pattern, .. } => covered_variants(pattern),
        _ => Vec::new(),
    }
}

/// `Foo.Bar.extractor` / `IntegerLiteral.extractor` → variant head immediately before `extractor`.
fn variant_head_before_extractor(path: &NamePath) -> Option<Identifier> {
    let parts = path.parts();
    if parts.len() >= 2 && parts.last().is_some_and(|part| part.as_str() == "extractor") { parts.get(parts.len() - 2).cloned() } else { None }
}

/// Minimal visibility enforcement for fields and methods.
#[derive(Debug, Default)]
pub struct VisibilityChecker {
    errors: Vec<AccessControlError>,
    classes: BTreeMap<Identifier, HirStruct>,
    inheritance_map: BTreeMap<Identifier, Vec<Identifier>>,
    singleton_names: BTreeSet<Identifier>,
    members: BTreeMap<(Identifier, Identifier), (HirVisibility, Vec<Identifier>)>,
}

impl VisibilityChecker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check_module(&mut self, module: &HirModule) -> Vec<AccessControlError> {
        self.errors.clear();
        self.classes.clear();
        self.inheritance_map.clear();
        self.singleton_names.clear();
        self.members.clear();

        for singleton in &module.singletons {
            self.singleton_names.insert(singleton.name.clone());
        }

        for class in &module.structs {
            self.classes.insert(class.name.clone(), class.clone());
            self.inheritance_map.insert(class.name.clone(), class.parents.iter().filter_map(|parent| last_name(&parent.name)).collect());
            for field in &class.fields {
                self.members.insert((class.name.clone(), field.name.clone()), (field.visibility.clone(), class.namespace.clone()));
            }
            for method in &class.methods {
                self.members.insert((class.name.clone(), method.name.clone()), (method.visibility.clone(), class.namespace.clone()));
            }
        }

        for function in &module.functions {
            let module_path = NamePath::new(vec![]);
            let context = AccessContext::new(module_path);
            let mut env = type_env_from_params(function);
            self.walk_block(&function.body, &context, &mut env);
        }
        for class in &module.structs {
            for method in &class.methods {
                let module_path = NamePath::new(class.namespace.clone());
                let context = AccessContext::in_method(module_path, class.name.clone(), method.name.clone());
                let mut env = type_env_from_params(method);
                env.insert(Identifier::new("self"), ValkyrieType::Named(class.name.clone()));
                self.walk_block(&method.body, &context, &mut env);
            }
        }
        for singleton in &module.singletons {
            for field in &singleton.fields {
                self.members.insert((singleton.name.clone(), field.name.clone()), (field.visibility.clone(), singleton.namespace.clone()));
            }
            for method in &singleton.methods {
                self.members.insert((singleton.name.clone(), method.name.clone()), (method.visibility.clone(), singleton.namespace.clone()));
            }
            for method in &singleton.methods {
                let module_path = NamePath::new(singleton.namespace.clone());
                let context = AccessContext::in_method(module_path, singleton.name.clone(), method.name.clone());
                let mut env = type_env_from_params(method);
                env.insert(Identifier::new("self"), ValkyrieType::Named(singleton.name.clone()));
                self.walk_block(&method.body, &context, &mut env);
            }
        }

        self.errors.clone()
    }

    pub fn check_member_access(
        &mut self,
        owner: &Identifier,
        member: &Identifier,
        visibility: &HirVisibility,
        owner_namespace: &[Identifier],
        context: &AccessContext,
    ) {
        match visibility.access {
            AccessLevel::Public => {}
            AccessLevel::Private => {
                if context.current_class.as_ref() != Some(owner) {
                    self.errors.push(AccessControlError::private_member_access(
                        owner.clone(),
                        member.clone(),
                        context.current_class.clone(),
                        None,
                    ));
                }
            }
            AccessLevel::Protected => {
                let allowed = context.current_class.as_ref().is_some_and(|current| current == owner || self.is_subclass_of(current, owner));
                if !allowed {
                    self.errors.push(AccessControlError::protected_member_access(
                        owner.clone(),
                        member.clone(),
                        context.current_class.clone().unwrap_or_else(|| Identifier::new("<module>")),
                        None,
                    ));
                }
            }
            AccessLevel::Internal => {
                let current = context.current_module.parts();
                if current != owner_namespace {
                    self.errors.push(AccessControlError::internal_member_access(
                        member.clone(),
                        NamePath::new(owner_namespace.to_vec()),
                        context.current_module.clone(),
                        None,
                    ));
                }
            }
        }
    }

    pub fn errors(&self) -> &[AccessControlError] {
        &self.errors
    }

    fn is_subclass_of(&self, child: &Identifier, parent: &Identifier) -> bool {
        let mut stack = self.inheritance_map.get(child).cloned().unwrap_or_default();
        let mut seen = BTreeSet::new();
        while let Some(next) = stack.pop() {
            if !seen.insert(next.clone()) {
                continue;
            }
            if &next == parent {
                return true;
            }
            if let Some(parents) = self.inheritance_map.get(&next) {
                stack.extend(parents.iter().cloned());
            }
        }
        false
    }

    fn walk_block(&mut self, block: &HirBlock, context: &AccessContext, env: &mut BTreeMap<Identifier, ValkyrieType>) {
        for statement in &block.statements {
            self.walk_statement(statement, context, env);
        }
        if let Some(expr) = &block.expr {
            self.walk_expr(expr, context, env);
        }
    }

    fn walk_statement(&mut self, statement: &HirStatement, context: &AccessContext, env: &mut BTreeMap<Identifier, ValkyrieType>) {
        match &statement.kind {
            HirStatementKind::Let { pattern, initializer, ty, .. } => {
                if let Some(value) = initializer {
                    self.walk_expr(value, context, env);
                }
                if let Some(ty) = ty {
                    if let HirPattern::Variable(name) = pattern {
                        env.insert(name.name.clone(), ty.clone());
                    }
                }
            }
            HirStatementKind::Expr(expr) => self.walk_expr(expr, context, env),
        }
    }

    fn walk_expr(&mut self, expr: &HirExpr, context: &AccessContext, env: &mut BTreeMap<Identifier, ValkyrieType>) {
        match &expr.kind {
            HirExprKind::FieldAccess { object, field } | HirExprKind::StoreField { object, field, .. } => {
                self.walk_expr(object, context, env);
                if let HirExprKind::StoreField { value, .. } = &expr.kind {
                    self.walk_expr(value, context, env);
                }
                if let Some(owner) = resolve_owner_type(object, env, &self.singleton_names) {
                    if let Some((visibility, namespace)) = self.members.get(&(owner.clone(), field.clone())).cloned() {
                        self.check_member_access(&owner, field, &visibility, &namespace, context);
                    }
                }
            }
            HirExprKind::Call { callee, args, .. } => {
                if let HirExprKind::FieldAccess { object, field } = &callee.kind {
                    self.walk_expr(object, context, env);
                    if let Some(owner) = resolve_owner_type(object, env, &self.singleton_names) {
                        if let Some((visibility, namespace)) = self.members.get(&(owner.clone(), field.clone())).cloned() {
                            self.check_member_access(&owner, field, &visibility, &namespace, context);
                        }
                    }
                }
                else {
                    self.walk_expr(callee, context, env);
                }
                for arg in crate::types::hir::hir_call_arg_values(args) {
                    self.walk_expr(arg, context, env);
                }
            }
            HirExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee, context, env);
                for arm in arms {
                    self.walk_expr(&arm.body, context, env);
                }
            }
            HirExprKind::If { condition, then_branch, else_branch }
            | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
                self.walk_expr(condition, context, env);
                self.walk_block(then_branch, context, env);
                if let Some(else_branch) = else_branch {
                    self.walk_block(else_branch, context, env);
                }
            }
            HirExprKind::Block(block) => self.walk_block(block, context, env),
            _ => {}
        }
    }
}

fn resolve_owner_type(
    object: &HirExpr,
    env: &BTreeMap<Identifier, ValkyrieType>,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<Identifier> {
    match &object.kind {
        HirExprKind::Variable(identifier) if singleton_names.contains(&identifier.name) => Some(identifier.name.clone()),
        HirExprKind::Path(path) if path.parts().len() == 1 && singleton_names.contains(&path.parts()[0]) => Some(path.parts()[0].clone()),
        HirExprKind::Variable(identifier) => match env.get(&identifier.name)? {
            ValkyrieType::Named(name) => Some(name.clone()),
            _ => None,
        },
        HirExprKind::Path(path) => last_name(path),
        _ => None,
    }
}
