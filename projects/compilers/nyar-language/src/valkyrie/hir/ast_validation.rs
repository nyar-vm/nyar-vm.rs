//! `AST -> HIR` 前的结构与语义预校验。
use std::ops::Range;

use crate::types::Identifier;
use std_data::text::valkyrie::{
    Annotations, ClassDeclaration, DeclarationBody, FunctionDeclaration, FunctionParameter, FunctionStatement, GenericParameterDeclaration,
    ImplyDeclaration, ObjectBody, ObjectFieldDeclaration, ObjectMethodDeclaration, ParseError, RootStatement, SumTypeKind, TermExpression,
    TraitAssociatedConstDeclaration, TraitAssociatedTypeDeclaration, TraitDeclaration, UniteDeclaration, ValkyrieRoot,
    ast::{ArmStatement, SubscriptItem},
};

use super::validate_type_expression;

/// 校验 `AST` 根节点是否满足进入 `HIR` lowering 的前提。
pub(crate) fn validate_ast_root(root: &ValkyrieRoot) -> Result<(), ParseError> {
    validate_unique_type_definition_names(root)?;
    for statement in &root.statements {
        validate_declaration_statement(statement)?;
    }
    Ok(())
}

fn validate_unique_type_definition_names(root: &ValkyrieRoot) -> Result<(), ParseError> {
    use std::collections::BTreeMap;

    // `namespace a.b;` (semicolon form) is a file-level directive with empty body;
    // subsequent top-level type defs belong to that namespace. Key uniqueness by
    // `(namespace, name)` so `std.data.text.msil.TextSpan` ≠ `core::text.TextSpan`.
    let mut current_namespace = String::new();
    let mut seen: BTreeMap<(String, String), &'static str> = BTreeMap::new();
    for statement in &root.statements {
        if let RootStatement::Namespace(namespace) = statement {
            if namespace.body.is_none() {
                current_namespace = namespace.name.parts.join(".");
            }
            continue;
        }
        let (kind, name, span) = match statement {
            RootStatement::TypeAlias(alias) => ("type alias", alias.name.name.as_str(), alias.name.span.clone()),
            RootStatement::Unite(unite) => ("unite", unite.name.name.as_str(), unite.name.span.clone()),
            RootStatement::Class(class_decl) => ("type", class_decl.name.name.as_str(), class_decl.name.span.clone()),
            RootStatement::Flags(flags) => ("flags", flags.name.name.as_str(), flags.name.span.clone()),
            RootStatement::Trait(trait_decl) => ("trait", trait_decl.name.name.as_str(), trait_decl.name.span.clone()),
            _ => continue,
        };
        let key = (current_namespace.clone(), name.to_string());
        if let Some(prev_kind) = seen.insert(key, kind) {
            let qualified = if current_namespace.is_empty() { name.to_string() } else { format!("{current_namespace}.{name}") };
            return Err(ParseError::invalid_at(format!("duplicate definition of `{qualified}`: {prev_kind} and {kind}"), span));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
struct ValidationContext {
    loop_labels: Vec<Option<Identifier>>,
    in_catch_arm: bool,
    in_match_arm: bool,
}

fn validate_declaration_statement(statement: &RootStatement) -> Result<(), ParseError> {
    match statement {
        RootStatement::Namespace(namespace) => {
            if let Some(body) = &namespace.body {
                validate_declaration_body(body)?;
            }
        }
        RootStatement::Using(_) => {}
        RootStatement::Function(function) => validate_function_declaration(function)?,
        RootStatement::Class(class_decl) => validate_class_declaration(class_decl)?,
        RootStatement::Trait(trait_decl) => validate_trait_declaration(trait_decl)?,
        RootStatement::Imply(imply_decl) => validate_imply_declaration(imply_decl)?,
        RootStatement::Unite(unite_decl) => validate_unite_declaration(unite_decl)?,
        RootStatement::Attribute(_) => {}
        RootStatement::TypeAlias(_) => {}
        RootStatement::Flags(flags) => {
            for member in &flags.members {
                if let Some(value) = &member.value {
                    validate_term_expression(value)?;
                }
            }
        }
        RootStatement::MacroAssign(macro_assign) => validate_term_expression(&macro_assign.value)?,
        RootStatement::Tests(tests) => validate_declaration_body(&tests.body)?,
    }
    Ok(())
}

fn validate_function_declaration(function: &FunctionDeclaration) -> Result<(), ParseError> {
    for param in &function.params {
        validate_function_parameter(param)?;
    }
    if let Some(return_type) = &function.return_type {
        validate_type_expression(return_type)?;
    }
    if let Some(body) = &function.body {
        let mut context = ValidationContext::default();
        validate_declaration_body_with_context(body, &mut context)?;
    }
    Ok(())
}

fn validate_class_declaration(class_decl: &ClassDeclaration) -> Result<(), ParseError> {
    for parent in &class_decl.inheritance {
        validate_type_expression(&parent.base_type)?;
    }
    validate_object_body(&class_decl.body)
}

fn validate_trait_declaration(trait_decl: &TraitDeclaration) -> Result<(), ParseError> {
    for parent in &trait_decl.inheritance {
        validate_type_expression(&parent.base_type)?;
    }
    for target in &trait_decl.alias_targets {
        validate_type_expression(&target.base_type)?;
    }
    validate_object_body(&trait_decl.body)
}

fn validate_imply_declaration(imply_decl: &ImplyDeclaration) -> Result<(), ParseError> {
    for parameter in &imply_decl.generic_parameters {
        validate_generic_parameter(parameter)?;
    }
    validate_type_expression(&imply_decl.target_type)?;
    if let Some(trait_type) = &imply_decl.trait_type {
        validate_type_expression(trait_type)?;
    }
    for constraint in &imply_decl.where_constraints {
        validate_type_expression(&constraint.target_type)?;
        for bound in &constraint.bounds {
            validate_type_expression(bound)?;
        }
    }
    for method in &imply_decl.methods {
        validate_object_method(method)?;
    }
    for binding in &imply_decl.associated_type_bindings {
        for parameter in &binding.generic_parameters {
            validate_generic_parameter(parameter)?;
        }
        validate_type_expression(&binding.concrete_type)?;
    }
    for binding in &imply_decl.associated_const_bindings {
        if let Some(const_type) = &binding.const_type {
            validate_type_expression(const_type)?;
        }
        validate_term_expression(&binding.value)?;
    }
    Ok(())
}

fn annotations_have_tag(annotations: &Annotations) -> bool {
    annotations.attributes().any(|attribute| attribute.name.parts.last().is_some_and(|name| name == "tag"))
}

fn validate_unite_declaration(unite_decl: &UniteDeclaration) -> Result<(), ParseError> {
    for variant in &unite_decl.variants {
        for field in &variant.fields {
            validate_object_field(field)?;
        }
        if let Some(result_type) = &variant.result_type {
            validate_type_expression(result_type)?;
        }
    }
    // `[tag(...)]` is unite-only. Enums use `Variant = N` or auto-increment.
    if unite_decl.kind == SumTypeKind::Enum {
        if annotations_have_tag(&unite_decl.annotations) {
            return Err(ParseError::invalid(format!(
                "`[tag(...)]` is only valid on `unite`, not `enums {}`; use `Variant = N` for explicit discriminators",
                unite_decl.name.name.as_str()
            )));
        }
        for variant in &unite_decl.variants {
            if annotations_have_tag(&variant.annotations) {
                return Err(ParseError::invalid(format!(
                    "`[tag(...)]` is only valid on `unite` variants, not `enums {}::{}`; write `{} = N` instead",
                    unite_decl.name.name.as_str(),
                    variant.name.name.as_str(),
                    variant.name.name.as_str()
                )));
            }
        }
    }
    // Tag-only variants belong on `enums` (discriminator layout), not `unite` (payload family).
    if unite_decl.kind == SumTypeKind::Unite
        && !unite_decl.variants.is_empty()
        && unite_decl.variants.iter().all(|variant| variant.fields.is_empty() && variant.result_type.is_none())
    {
        return Err(ParseError::invalid(format!(
            "`unite {}` has only payload-less variants; use `enums {}` instead (`enums` and `unite` have different layouts)",
            unite_decl.name.name.as_str(),
            unite_decl.name.name.as_str()
        )));
    }
    Ok(())
}

fn validate_object_body(body: &ObjectBody) -> Result<(), ParseError> {
    for field in &body.fields {
        validate_object_field(field)?;
    }
    for method in &body.methods {
        validate_object_method(method)?;
    }
    for item in &body.associated_types {
        validate_trait_associated_type(item)?;
    }
    for item in &body.associated_constants {
        validate_trait_associated_const(item)?;
    }
    Ok(())
}

fn validate_object_field(field: &ObjectFieldDeclaration) -> Result<(), ParseError> {
    validate_type_expression(&field.field_type)?;
    if let Some(default_value) = &field.default_value {
        validate_term_expression(default_value)?;
    }
    Ok(())
}

fn validate_object_method(method: &ObjectMethodDeclaration) -> Result<(), ParseError> {
    for param in &method.params {
        validate_function_parameter(param)?;
    }
    if let Some(return_type) = &method.return_type {
        validate_type_expression(return_type)?;
    }
    if let Some(body) = &method.body {
        validate_declaration_body(body)?;
    }
    Ok(())
}

fn validate_trait_associated_type(item: &TraitAssociatedTypeDeclaration) -> Result<(), ParseError> {
    for bound in &item.bounds {
        validate_type_expression(bound)?;
    }
    if let Some(default_type) = &item.default_type {
        validate_type_expression(default_type)?;
    }
    Ok(())
}

fn validate_trait_associated_const(item: &TraitAssociatedConstDeclaration) -> Result<(), ParseError> {
    validate_type_expression(&item.const_type)?;
    if let Some(default_value) = &item.default_value {
        validate_term_expression(default_value)?;
    }
    Ok(())
}

fn validate_generic_parameter(parameter: &GenericParameterDeclaration) -> Result<(), ParseError> {
    for bound in &parameter.bounds {
        validate_type_expression(bound)?;
    }
    if let Some(default_type) = &parameter.default_type {
        validate_type_expression(default_type)?;
    }
    Ok(())
}

fn validate_function_parameter(param: &FunctionParameter) -> Result<(), ParseError> {
    if let Some(parameter_type) = &param.parameter_type {
        validate_type_expression(parameter_type)?;
    }
    Ok(())
}

fn validate_declaration_body(body: &DeclarationBody) -> Result<(), ParseError> {
    let mut context = ValidationContext::default();
    validate_declaration_body_with_context(body, &mut context)
}

fn validate_declaration_body_with_context(body: &DeclarationBody, context: &mut ValidationContext) -> Result<(), ParseError> {
    for statement in &body.statements {
        validate_statement_with_context(statement, context)?;
    }
    if let Some(tail_expression) = &body.tail_expression {
        validate_term_expression_with_context(tail_expression, context)?;
    }
    Ok(())
}

fn validate_statement_with_context(statement: &FunctionStatement, context: &mut ValidationContext) -> Result<(), ParseError> {
    match statement {
        FunctionStatement::Let(statement) => {
            if let Some(ty) = &statement.ty {
                validate_type_expression(ty)?;
            }
            if let Some(initializer) = &statement.initializer {
                validate_term_expression_with_context(initializer, context)?;
            }
        }
        FunctionStatement::Term { expression, .. } => validate_term_expression_with_context(expression, context)?,
        FunctionStatement::Function { function, .. } => validate_function_declaration(function)?,
        FunctionStatement::Break(statement) => {
            validate_loop_control_target("break", statement.label.as_ref(), &statement.span, context)?;
            if let Some(value) = &statement.value {
                validate_term_expression_with_context(value, context)?;
            }
        }
        FunctionStatement::Continue(statement) => {
            validate_loop_control_target("continue", statement.label.as_ref(), &statement.span, context)?;
        }
        FunctionStatement::Yield(statement) => {
            if let Some(value) = &statement.value {
                validate_term_expression_with_context(value, context)?;
            }
        }
        FunctionStatement::YieldFrom(statement) => validate_term_expression_with_context(&statement.value, context)?,
        FunctionStatement::Return(statement) => {
            if let Some(value) = &statement.value {
                validate_term_expression_with_context(value, context)?;
            }
        }
        FunctionStatement::Resume(statement) => {
            if !context.in_catch_arm {
                return Err(ParseError::invalid_at("`resume` 只允许出现在 `catch` 分支中", statement.span.clone()));
            }
            if let Some(value) = &statement.value {
                validate_term_expression_with_context(value, context)?;
            }
        }
        FunctionStatement::Fallthrough(statement) => {
            if !(context.in_match_arm && !context.in_catch_arm) {
                return Err(ParseError::invalid_at("`fallthrough` 仅允许出现在 `case` statement 体系中", statement.span.clone()));
            }
        }
    }
    Ok(())
}

fn validate_term_expression(expression: &TermExpression) -> Result<(), ParseError> {
    let mut context = ValidationContext::default();
    validate_term_expression_with_context(expression, &mut context)
}

fn validate_term_expression_with_context(expression: &TermExpression, context: &mut ValidationContext) -> Result<(), ParseError> {
    match expression {
        TermExpression::Name { .. } | TermExpression::Literal { .. } => {}
        TermExpression::Unary(term_unary) => validate_term_expression_with_context(&term_unary.base, context)?,
        TermExpression::Binary(term_binary) => {
            validate_term_expression_with_context(&term_binary.lhs, context)?;
            validate_term_expression_with_context(&term_binary.rhs, context)?;
        }
        TermExpression::Call(term_call) => {
            validate_term_expression_with_context(&term_call.callee, context)?;
            for arg in &term_call.args.arguments {
                validate_term_expression_with_context(&arg.value, context)?;
            }
        }
        TermExpression::DotCall(term_dot) => {
            validate_term_expression_with_context(&term_dot.base, context)?;
            for arg in &term_dot.arguments.arguments {
                validate_term_expression_with_context(&arg.value, context)?;
            }
        }
        TermExpression::Subscript(term_subscript) => {
            validate_term_expression_with_context(&term_subscript.base, context)?;
            for subscript in &term_subscript.subscripts {
                match subscript {
                    SubscriptItem::Index { term, .. } => validate_term_expression_with_context(term, context)?,
                    SubscriptItem::Slice { start, end, step, .. } => {
                        if let Some(start) = start {
                            validate_term_expression_with_context(start, context)?;
                        }
                        if let Some(end) = end {
                            validate_term_expression_with_context(end, context)?;
                        }
                        if let Some(step) = step {
                            validate_term_expression_with_context(step, context)?;
                        }
                    }
                }
            }
        }
        TermExpression::Dereference(term_dereference) => {
            validate_term_expression_with_context(&term_dereference.base, context)?;
        }
        TermExpression::Tuple { items, .. } | TermExpression::Array { items, .. } => {
            for item in items {
                validate_term_expression_with_context(item, context)?;
            }
        }
        TermExpression::As(term_as) => {
            validate_term_expression_with_context(&term_as.base, context)?;
            validate_type_expression(&term_as.target)?;
        }
        TermExpression::Is(term_is) => {
            validate_term_expression_with_context(&term_is.base, context)?;
        }
        TermExpression::Turbofish { expr, arguments, .. } => {
            validate_term_expression_with_context(expr, context)?;
            for argument in arguments {
                validate_type_expression(argument)?;
            }
        }
        TermExpression::Assign { target, value, .. } => {
            validate_term_expression_with_context(target, context)?;
            validate_term_expression_with_context(value, context)?;
        }
        TermExpression::Raise { value, .. } => validate_term_expression_with_context(value, context)?,
        TermExpression::TryPropagate { base, .. } => validate_term_expression_with_context(base, context)?,
        TermExpression::MacroInvoke { args, .. } => {
            for arg in args {
                validate_term_expression_with_context(arg, context)?;
            }
        }
        TermExpression::If(if_stmt) => {
            validate_term_expression_with_context(&if_stmt.condition, context)?;
            validate_declaration_body_with_context(&if_stmt.then_body, context)?;
            if let Some(else_body) = &if_stmt.else_body {
                validate_declaration_body_with_context(else_body, context)?;
            }
        }
        TermExpression::IfLet(if_let_stmt) => {
            validate_term_expression_with_context(&if_let_stmt.item, context)?;
            validate_declaration_body_with_context(&if_let_stmt.then_body, context)?;
            if let Some(else_body) = &if_let_stmt.else_body {
                validate_declaration_body_with_context(else_body, context)?;
            }
        }
        TermExpression::Loop(loop_stmt) => {
            context.loop_labels.push(None);
            let result = validate_declaration_body_with_context(&loop_stmt.body, context);
            context.loop_labels.pop();
            result?;
        }
        TermExpression::LoopIn(loop_in_stmt) => {
            if let Some(iterator) = &loop_in_stmt.iterator {
                validate_term_expression_with_context(iterator, context)?;
            }
            if let Some(condition) = &loop_in_stmt.condition {
                validate_term_expression_with_context(condition, context)?;
            }
            context.loop_labels.push(loop_in_stmt.label.clone());
            let result = validate_declaration_body_with_context(&loop_in_stmt.body, context);
            context.loop_labels.pop();
            result?;
        }
        TermExpression::While(while_stmt) => {
            if let Some(condition) = &while_stmt.condition {
                validate_term_expression_with_context(condition, context)?;
            }
            context.loop_labels.push(while_stmt.label.clone());
            let result = validate_declaration_body_with_context(&while_stmt.body, context);
            context.loop_labels.pop();
            result?;
        }
        TermExpression::WhileLet(while_let_stmt) => {
            validate_term_expression_with_context(&while_let_stmt.scrutinee, context)?;
            if let Some(guard) = &while_let_stmt.guard {
                validate_term_expression_with_context(guard, context)?;
            }
            context.loop_labels.push(while_let_stmt.label.clone());
            let result = validate_declaration_body_with_context(&while_let_stmt.body, context);
            context.loop_labels.pop();
            result?;
        }
        TermExpression::Until(until_stmt) => {
            if let Some(iterator) = &until_stmt.iterator {
                validate_term_expression_with_context(iterator, context)?;
            }
            if let Some(condition) = &until_stmt.condition {
                validate_term_expression_with_context(condition, context)?;
            }
            context.loop_labels.push(until_stmt.label.clone());
            let result = validate_declaration_body_with_context(&until_stmt.body, context);
            context.loop_labels.pop();
            result?;
        }
        TermExpression::UntilNot(until_not_stmt) => {
            if let Some(iterator) = &until_not_stmt.iterator {
                validate_term_expression_with_context(iterator, context)?;
            }
            if let Some(condition) = &until_not_stmt.condition {
                validate_term_expression_with_context(condition, context)?;
            }
            context.loop_labels.push(until_not_stmt.label.clone());
            let result = validate_declaration_body_with_context(&until_not_stmt.body, context);
            context.loop_labels.pop();
            result?;
        }
        TermExpression::Match { scrutinee, arms, .. } => {
            validate_term_expression_with_context(scrutinee, context)?;
            for arm in arms {
                validate_arm_statement(arm, context, false)?;
            }
        }
        TermExpression::Catch { expr, arms, .. } => {
            validate_term_expression_with_context(expr, context)?;
            for arm in arms {
                validate_arm_statement(arm, context, true)?;
            }
        }
        TermExpression::Construct { fields, .. } => {
            for (_, value) in fields {
                validate_term_expression_with_context(value, context)?;
            }
        }
        TermExpression::Lambda { body, .. } => {
            let mut lambda_context =
                ValidationContext { loop_labels: context.loop_labels.clone(), in_catch_arm: context.in_catch_arm, in_match_arm: false };
            validate_declaration_body_with_context(body, &mut lambda_context)?;
        }
        TermExpression::Try(try_stmt) => validate_declaration_body_with_context(&try_stmt.body, context)?,
        TermExpression::Block { body, .. } => {
            validate_declaration_body_with_context(body, context)?;
        }
        TermExpression::XmlMarkup { .. } | TermExpression::Template { .. } => {}
        TermExpression::PostfixMatch { .. } | TermExpression::PostfixCatch { .. } | TermExpression::AnonymousClass { .. } => {}
    }
    Ok(())
}

fn validate_arm_statement(arm: &ArmStatement, context: &mut ValidationContext, in_catch_arm: bool) -> Result<(), ParseError> {
    match arm {
        ArmStatement::Case(arm) => {
            if let Some(guard_expr) = &arm.guard {
                validate_term_expression_with_context(guard_expr, context)?;
            }
            let previous_in_match_arm = context.in_match_arm;
            let previous_in_catch_arm = context.in_catch_arm;
            context.in_match_arm = !in_catch_arm;
            context.in_catch_arm = in_catch_arm;
            let result = validate_declaration_body_with_context(&arm.body, context);
            context.in_match_arm = previous_in_match_arm;
            context.in_catch_arm = previous_in_catch_arm;
            result
        }
        ArmStatement::Type(arm) => {
            if let Some(guard_expr) = &arm.guard {
                validate_term_expression_with_context(guard_expr, context)?;
            }
            let previous_in_match_arm = context.in_match_arm;
            let previous_in_catch_arm = context.in_catch_arm;
            context.in_match_arm = !in_catch_arm;
            context.in_catch_arm = in_catch_arm;
            let result = validate_declaration_body_with_context(&arm.body, context);
            context.in_match_arm = previous_in_match_arm;
            context.in_catch_arm = previous_in_catch_arm;
            result
        }
        ArmStatement::Else(_) => Ok(()),
    }
}

fn validate_loop_control_target(
    keyword: &str,
    label: Option<&Identifier>,
    span: &Range<usize>,
    context: &ValidationContext,
) -> Result<(), ParseError> {
    if context.loop_labels.is_empty() {
        return Err(ParseError::invalid_at(format!("`{keyword}` 只能出现在循环内"), span.clone()));
    }
    if let Some(label) = label {
        if !context.loop_labels.iter().rev().any(|candidate| candidate.as_ref() == Some(label)) {
            return Err(ParseError::invalid_at(format!("未找到 label `{label}` 对应的循环 region"), span.clone()));
        }
    }
    Ok(())
}
