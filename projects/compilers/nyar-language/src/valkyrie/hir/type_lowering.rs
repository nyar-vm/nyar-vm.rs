//! Oak `AST` 类型表达式到 `HIR` 类型的 lowering 与预检查。
use std::{cell::RefCell, collections::BTreeSet};

use crate::{
    types::{
        Identifier,
        hir::{FunctionType, RowMethodType, RowType, ValkyrieType},
    },
    valkyrie::frontend::{
        self, ValkyrieRoot,
        ast::{
            NamePath as AstNamePath, TypeExpression,
            type_nodes::{FunctionType as AstFunctionType, OptionalType, TupleType},
        },
    },
};
use oak_valkyrie::lexer::token_type::ValkyrieTokenType;
use crate::valkyrie::frontend::ParseError;

thread_local! {
    static SHADOWED_BUILTIN_TYPE_ALIASES: RefCell<Vec<BTreeSet<String>>> = RefCell::new(Vec::new());
    static MODULE_TYPE_ALIASES: RefCell<Vec<BTreeMap<String, ModuleTypeAliasEntry>>> = RefCell::new(Vec::new());
}

use std::collections::BTreeMap;

/// 模块类型别名条目：可携带泛型形参。
#[derive(Debug, Clone)]
struct ModuleTypeAliasEntry {
    params: Vec<String>,
    target: ValkyrieType,
}

/// 管理当前 lowering 过程中的内建类型别名遮蔽作用域。
#[derive(Debug)]
pub(crate) struct BuiltinTypeAliasScope;

impl BuiltinTypeAliasScope {
    /// 进入一次新的根 lowering 作用域。
    pub(crate) fn enter(root: &ValkyrieRoot) -> Self {
        let aliases = collect_shadowed_builtin_type_aliases(root);
        SHADOWED_BUILTIN_TYPE_ALIASES.with(|stack| {
            stack.borrow_mut().push(aliases);
        });
        Self
    }
}

impl Drop for BuiltinTypeAliasScope {
    fn drop(&mut self) {
        SHADOWED_BUILTIN_TYPE_ALIASES.with(|stack| {
            let _ = stack.borrow_mut().pop();
        });
    }
}

/// 管理当前 lowering 过程中的模块 `type` 别名作用域。
#[derive(Debug)]
pub(crate) struct ModuleTypeAliasScope;

impl ModuleTypeAliasScope {
    /// 进入一次新的根 lowering 作用域。
    pub(crate) fn enter_empty() -> Self {
        MODULE_TYPE_ALIASES.with(|stack| {
            stack.borrow_mut().push(BTreeMap::new());
        });
        Self
    }

    /// 注册一个模块类型别名到当前作用域。
    pub(crate) fn register_alias(name: &str, params: Vec<String>, target: ValkyrieType) {
        MODULE_TYPE_ALIASES.with(|stack| {
            if let Some(top) = stack.borrow_mut().last_mut() {
                top.insert(name.to_string(), ModuleTypeAliasEntry { params, target });
            }
        });
    }
}

impl Drop for ModuleTypeAliasScope {
    fn drop(&mut self) {
        MODULE_TYPE_ALIASES.with(|stack| {
            let _ = stack.borrow_mut().pop();
        });
    }
}

/// 校验 Oak `AST` 类型表达式是否满足当前 `HIR` lowering 前提。
pub(crate) fn validate_type_expression(ty: &TypeExpression) -> Result<(), ParseError> {
    match ty {
        TypeExpression::Namepath(path) => {
            if let Some(part) = path.parts.last() {
                validate_source_text_type_name(&part.name)?;
                if let Some(canonical_name) = legacy_builtin_type_alias(&part.name) {
                    return Err(ParseError::invalid(format!(
                        "legacy builtin type alias `{name}` has been removed; use `{canonical_name}` explicitly",
                        name = part.name
                    )));
                }
            }
        }
        TypeExpression::Generic(generic) => {
            validate_source_text_type_name(&generic.name.name)?;
        }
        TypeExpression::Tuple(tuple) => {
            for element in &tuple.elements {
                validate_type_expression(element)?;
            }
        }
        TypeExpression::Function(function) => {
            for param in &function.params {
                validate_type_expression(param)?;
            }
            validate_type_expression(&function.return_type)?;
        }
        TypeExpression::Optional(optional) => validate_type_expression(&optional.inner)?,
        TypeExpression::AssociatedType(associated) => {
            validate_source_text_type_name(&associated.name.name)?;
            validate_source_text_type_name(&associated.base.name)?;
        }
        TypeExpression::QualifiedAssociatedType(qualified) => {
            validate_type_expression(&qualified.ty)?;
            validate_source_text_type_name(&qualified.name.name)?;
        }
        TypeExpression::Binary(node) => {
            validate_type_expression(&node.lhs)?;
            validate_type_expression(&node.rhs)?;
        }
        TypeExpression::Unary(node) => validate_type_expression(&node.base)?,
    }
    Ok(())
}

/// Reject source-level text aliases that do not identify a language encoding.
pub fn validate_source_text_type_name(name: &str) -> Result<(), ParseError> {
    if is_legacy_text_type_name(name) {
        return Err(ParseError::invalid(format!(
            "ambiguous text type `{name}` is forbidden; use an explicit encoding such as `utf8`, `utf16`, or `c_str`"
        )));
    }
    Ok(())
}

/// 将 Oak `AST` 类型表达式降到最小 `HIR` 类型表示。
pub(crate) fn lower_type_expression(ty: &TypeExpression) -> ValkyrieType {
    match ty {
        TypeExpression::Namepath(path) => lower_type_namepath(path),
        TypeExpression::Generic(generic) => ValkyrieType::Named(Identifier::new(&generic.name.name)),
        TypeExpression::Tuple(tuple) => {
            if tuple.elements.is_empty() {
                ValkyrieType::Unit
            }
            else {
                ValkyrieType::Tuple(tuple.elements.iter().map(lower_type_expression).collect())
            }
        }
        TypeExpression::Function(function) => ValkyrieType::Function(Box::new(FunctionType {
            params: function.params.iter().map(lower_type_expression).collect(),
            return_type: lower_type_expression(&function.return_type),
        })),
        TypeExpression::Optional(optional) => flatten_nullable_type(lower_type_expression(&optional.inner)),
        TypeExpression::AssociatedType(associated) => ValkyrieType::Associated(Box::new(crate::types::hir::AssociatedType {
            base: if associated.base.name == "Self" {
                ValkyrieType::SelfType
            }
            else {
                ValkyrieType::Named(Identifier::new(&associated.base.name))
            },
            name: Identifier::new(&associated.name.name),
            type_arguments: Vec::new(),
        })),
        TypeExpression::QualifiedAssociatedType(qualified) => {
            let base = lower_type_expression(&qualified.ty);
            ValkyrieType::Associated(Box::new(crate::types::hir::AssociatedType {
                base,
                name: Identifier::new(&qualified.name.name),
                type_arguments: qualified.trait_path.parts.iter().map(|part| ValkyrieType::Named(Identifier::new(&part.name))).collect(),
            }))
        }
        TypeExpression::Binary(node) => match node.operator {
            ValkyrieTokenType::Pipe => ValkyrieType::Union(vec![lower_type_expression(&node.lhs), lower_type_expression(&node.rhs)]),
            ValkyrieTokenType::Ampersand => {
                ValkyrieType::Intersection(vec![lower_type_expression(&node.lhs), lower_type_expression(&node.rhs)])
            }
            _ => lower_type_expression(&node.lhs),
        },
        TypeExpression::Unary(node) => match node.operator {
            ValkyrieTokenType::LeftBracket => ValkyrieType::Array(Box::new(lower_type_expression(&node.base))),
            _ => lower_type_expression(&node.base),
        },
    }
}

fn flatten_nullable_type(inner: ValkyrieType) -> ValkyrieType {
    match inner {
        ValkyrieType::Nullable(payload) => ValkyrieType::Nullable(payload),
        payload => ValkyrieType::Nullable(Box::new(payload)),
    }
}

/// 渲染类型表达式，供错误消息与回退路径使用。
pub(crate) fn render_type_expression(ty: &TypeExpression) -> String {
    match ty {
        TypeExpression::Namepath(path) => path.parts.iter().map(|part| part.name.as_str()).collect::<Vec<_>>().join("::"),
        TypeExpression::Generic(generic) => generic.name.name.clone(),
        TypeExpression::Tuple(tuple) => {
            if tuple.elements.is_empty() {
                return "()".to_string();
            }
            let inner = tuple.elements.iter().map(render_type_expression).collect::<Vec<_>>().join(", ");
            format!("({inner})")
        }
        TypeExpression::Function(function) => {
            let params_str = function.params.iter().map(render_type_expression).collect::<Vec<_>>().join(", ");
            format!("micro({params_str}) -> {}", render_type_expression(&function.return_type))
        }
        TypeExpression::Optional(optional) => {
            format!("{}?", render_type_expression(&optional.inner))
        }
        TypeExpression::AssociatedType(associated) => {
            format!("{}::{}", associated.base.name, associated.name.name)
        }
        TypeExpression::QualifiedAssociatedType(qualified) => {
            format!(
                "<{} as {}>::{}",
                render_type_expression(&qualified.ty),
                render_type_expression(&TypeExpression::Namepath(Box::new(qualified.trait_path.clone()))),
                qualified.name.name
            )
        }
        TypeExpression::Binary(node) => {
            let op = match node.operator {
                ValkyrieTokenType::Pipe => " | ",
                ValkyrieTokenType::Ampersand => " & ",
                _ => " ",
            };
            format!("{}{}{}", render_type_expression(&node.lhs), op, render_type_expression(&node.rhs))
        }
        TypeExpression::Unary(node) => match node.operator {
            ValkyrieTokenType::LeftBracket => format!("[{}]", render_type_expression(&node.base)),
            _ => render_type_expression(&node.base),
        },
    }
}

fn is_legacy_text_type_name(name: &str) -> bool {
    matches!(name, "string" | "str" | "String")
}

fn legacy_builtin_type_alias(name: &str) -> Option<&'static str> {
    match name {
        "sbyte" => Some("i8"),
        "short" => Some("i16"),
        "int" => Some("i32"),
        "long" => Some("i64"),
        "byte" => Some("u8"),
        "ushort" => Some("u16"),
        "uint" => Some("u32"),
        "ulong" => Some("u64"),
        "float" => Some("f32"),
        "double" => Some("f64"),
        "boolean" => Some("bool"),
        _ => None,
    }
}

fn canonical_builtin_type(name: &str) -> Option<ValkyrieType> {
    match name {
        "i8" => Some(ValkyrieType::Integer8 { signed: true }),
        "i16" => Some(ValkyrieType::Integer16 { signed: true }),
        "i32" => Some(ValkyrieType::Integer32 { signed: true }),
        "i64" => Some(ValkyrieType::Integer64 { signed: true }),
        "u8" => Some(ValkyrieType::Integer8 { signed: false }),
        "u16" => Some(ValkyrieType::Integer16 { signed: false }),
        "u32" => Some(ValkyrieType::Integer32 { signed: false }),
        "u64" => Some(ValkyrieType::Integer64 { signed: false }),
        "f32" => Some(ValkyrieType::Float32),
        "f64" => Some(ValkyrieType::Float64),
        "bool" => Some(ValkyrieType::Boolean),
        "char" => Some(ValkyrieType::Character),
        "utf8" => Some(ValkyrieType::Utf8),
        "utf16" => Some(ValkyrieType::Utf16),
        "c_str" => Some(ValkyrieType::Named(Identifier::new("c_str"))),
        "unit" => Some(ValkyrieType::Unit),
        "void" => Some(ValkyrieType::Void),
        _ => None,
    }
}

fn collect_shadowed_builtin_type_aliases(_root: &ValkyrieRoot) -> BTreeSet<String> {
    BTreeSet::new()
}

fn is_shadowed_builtin_type(name: &str) -> bool {
    SHADOWED_BUILTIN_TYPE_ALIASES.with(|stack| stack.borrow().last().is_some_and(|aliases| aliases.contains(name)))
}

fn expand_module_type_alias(name: &str, arguments: &[ValkyrieType], visiting: &mut BTreeSet<String>) -> Option<ValkyrieType> {
    if visiting.contains(name) {
        return None;
    }
    MODULE_TYPE_ALIASES.with(|stack| {
        let entry = stack.borrow().last().and_then(|aliases| aliases.get(name).cloned())?;
        if entry.params.len() != arguments.len() {
            return None;
        }
        visiting.insert(name.to_string());
        let substituted = if entry.params.is_empty() {
            entry.target.clone()
        }
        else {
            let mapping: BTreeMap<String, ValkyrieType> = entry.params.into_iter().zip(arguments.iter().cloned()).collect();
            substitute_named_type_params(&entry.target, &mapping)
        };
        let expanded = expand_type_aliases(&substituted, visiting);
        visiting.remove(name);
        Some(expanded)
    })
}

fn substitute_named_type_params(ty: &ValkyrieType, mapping: &BTreeMap<String, ValkyrieType>) -> ValkyrieType {
    match ty {
        ValkyrieType::Named(name) => mapping.get(name.as_str()).cloned().unwrap_or_else(|| ty.clone()),
        ValkyrieType::Apply(base, args) => ValkyrieType::Apply(
            Box::new(substitute_named_type_params(base, mapping)),
            args.iter().map(|arg| substitute_named_type_params(arg, mapping)).collect(),
        ),
        ValkyrieType::Array(inner) => ValkyrieType::Array(Box::new(substitute_named_type_params(inner, mapping))),
        ValkyrieType::Nullable(inner) => ValkyrieType::Nullable(Box::new(substitute_named_type_params(inner, mapping))),
        ValkyrieType::Tuple(items) => ValkyrieType::Tuple(items.iter().map(|item| substitute_named_type_params(item, mapping)).collect()),
        ValkyrieType::Union(items) => ValkyrieType::Union(items.iter().map(|item| substitute_named_type_params(item, mapping)).collect()),
        ValkyrieType::Intersection(items) => {
            ValkyrieType::Intersection(items.iter().map(|item| substitute_named_type_params(item, mapping)).collect())
        }
        ValkyrieType::Function(function) => ValkyrieType::Function(Box::new(crate::types::hir::FunctionType {
            params: function.params.iter().map(|param| substitute_named_type_params(param, mapping)).collect(),
            return_type: substitute_named_type_params(&function.return_type, mapping),
        })),
        _ => ty.clone(),
    }
}

fn expand_type_aliases(ty: &ValkyrieType, visiting: &mut BTreeSet<String>) -> ValkyrieType {
    match ty {
        ValkyrieType::Named(name) => {
            if let Some(expanded) = expand_module_type_alias(name.as_str(), &[], visiting) {
                expanded
            }
            else {
                ty.clone()
            }
        }
        ValkyrieType::Apply(base, args) => {
            let expanded_args: Vec<ValkyrieType> = args.iter().map(|arg| expand_type_aliases(arg, visiting)).collect();
            if let ValkyrieType::Named(name) = base.as_ref() {
                if let Some(expanded) = expand_module_type_alias(name.as_str(), &expanded_args, visiting) {
                    return expanded;
                }
            }
            ValkyrieType::Apply(Box::new(expand_type_aliases(base, visiting)), expanded_args)
        }
        ValkyrieType::Array(inner) => ValkyrieType::Array(Box::new(expand_type_aliases(inner, visiting))),
        ValkyrieType::Nullable(inner) => ValkyrieType::Nullable(Box::new(expand_type_aliases(inner, visiting))),
        ValkyrieType::Tuple(items) => ValkyrieType::Tuple(items.iter().map(|item| expand_type_aliases(item, visiting)).collect()),
        ValkyrieType::Union(items) => ValkyrieType::Union(items.iter().map(|item| expand_type_aliases(item, visiting)).collect()),
        ValkyrieType::Intersection(items) => ValkyrieType::Intersection(items.iter().map(|item| expand_type_aliases(item, visiting)).collect()),
        ValkyrieType::Function(function) => ValkyrieType::Function(Box::new(crate::types::hir::FunctionType {
            params: function.params.iter().map(|param| expand_type_aliases(param, visiting)).collect(),
            return_type: expand_type_aliases(&function.return_type, visiting),
        })),
        _ => ty.clone(),
    }
}

fn lower_type_namepath(path: &AstNamePath) -> ValkyrieType {
    let parts = path.parts.iter().map(|part| part.name.as_str()).collect::<Vec<_>>();
    let last = parts.last().cloned().unwrap_or_default();
    if parts.len() == 1 && last == "Self" {
        return ValkyrieType::SelfType;
    }
    if parts.first().is_some_and(|part| *part == "Self") {
        let mut base = ValkyrieType::SelfType;
        for (index, name) in parts.iter().enumerate().skip(1) {
            base = ValkyrieType::Associated(Box::new(crate::types::hir::AssociatedType {
                base,
                name: Identifier::new(name),
                type_arguments: if index + 1 == parts.len() { Vec::new() } else { Vec::new() },
            }));
        }
        return base;
    }
    let base = if !is_shadowed_builtin_type(last) && (parts.len() == 1 || is_known_builtin_type_namespace(&parts, last)) {
        canonical_builtin_type(last).unwrap_or_else(|| ValkyrieType::Named(Identifier::new(last)))
    }
    else {
        ValkyrieType::Named(Identifier::new(last))
    };
    if parts.len() == 1 {
        if let Some(expanded) = expand_module_type_alias(last, &[], &mut BTreeSet::new()) {
            return expanded;
        }
    }
    if last == "Array" {
        return ValkyrieType::Array(Box::new(ValkyrieType::AutoType));
    }
    base
}

fn is_known_builtin_type_namespace(parts: &[&str], last: &str) -> bool {
    let prefix = &parts[..parts.len().saturating_sub(1)];
    match prefix {
        ["core", "primitive"] => canonical_builtin_type(last).is_some(),
        ["core", "text"] => matches!(last, "char" | "utf8" | "utf16" | "c_str"),
        ["std", "text"] => matches!(last, "utf8" | "utf16" | "c_str"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        types::Identifier as HirIdentifier,
        valkyrie::frontend::ast::{Identifier, NamePath, type_nodes::TypeUnaryNode},
    };
    use oak_core::Range;

    fn oak_span(len: usize) -> Range<usize> {
        Range { start: 0, end: len }
    }

    fn path_type(name: &str) -> TypeExpression {
        TypeExpression::Namepath(Box::new(NamePath {
            parts: vec![Identifier { name: name.to_string(), span: oak_span(name.len()) }],
            span: oak_span(name.len()),
        }))
    }

    #[test]
    fn rejects_ambiguous_string_type_names() {
        for name in ["string", "str", "String"] {
            let error = validate_type_expression(&path_type(name)).expect_err("ambiguous string types must be rejected");
            let message = error.to_string();
            assert!(message.contains("ambiguous text type") || message.contains("forbidden"), "message={message}");
            assert!(message.contains("utf8") && message.contains("utf16") && message.contains("c_str"), "message={message}");
        }
    }

    #[test]
    fn accepts_explicit_text_encodings() {
        for name in ["utf8", "utf16", "c_str"] {
            validate_type_expression(&path_type(name)).expect("explicit text encodings must be accepted");
        }
        assert_eq!(canonical_builtin_type("utf8"), Some(ValkyrieType::Utf8));
        assert_eq!(canonical_builtin_type("utf16"), Some(ValkyrieType::Utf16));
        assert_eq!(canonical_builtin_type("c_str"), Some(ValkyrieType::Named(HirIdentifier::new("c_str"))));
        assert!(canonical_builtin_type("string").is_none());
    }

    #[test]
    fn bracket_sugar_lowers_to_array() {
        let sugar = TypeExpression::Unary(Box::new(TypeUnaryNode {
            operator: ValkyrieTokenType::LeftBracket,
            base: path_type("i32"),
            span: oak_span(0),
        }));
        let expected = ValkyrieType::Array(Box::new(ValkyrieType::Integer32 { signed: true }));
        assert_eq!(lower_type_expression(&sugar), expected);
    }
}
