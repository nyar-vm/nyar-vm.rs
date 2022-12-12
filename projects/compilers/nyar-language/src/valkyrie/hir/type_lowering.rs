//! `AST` 类型表达式到 `HIR` 类型的 lowering 与预检查。
use std::{cell::RefCell, collections::BTreeSet};

use crate::types::{
    Identifier,
    hir::{FunctionType, RowMethodType, RowType, ValkyrieType},
};
use std_data::text::valkyrie::{ParseError, RootStatement, TypeExpression, ValkyrieRoot, ast::TypePath as AstTypePath};

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

/// 校验前端 `AST` 类型表达式是否满足当前 `HIR` lowering 前提。
pub(crate) fn validate_type_expression(ty: &TypeExpression) -> Result<(), ParseError> {
    match ty {
        TypeExpression::Path(path) => {
            if let Some(name) = path.name.parts.last() {
                validate_source_text_type_name(name)?;
                if let Some(canonical_name) = legacy_builtin_type_alias(name) {
                    return Err(ParseError::invalid(format!(
                        "legacy builtin type alias `{name}` has been removed; use `{canonical_name}` explicitly"
                    )));
                }
            }
            for argument in &path.arguments {
                validate_type_expression(argument)?;
            }
        }
        TypeExpression::Array { item, .. } => validate_type_expression(item)?,
        TypeExpression::FixedArray { item, .. } => validate_type_expression(item)?,
        TypeExpression::Tuple { items, .. } => {
            for item in items {
                validate_type_expression(item)?;
            }
        }
        TypeExpression::Pointer { item, .. } => validate_type_expression(item)?,
        TypeExpression::Row { methods, .. } => {
            for method in methods {
                for param in &method.params {
                    validate_type_expression(param)?;
                }
                validate_type_expression(&method.return_type)?;
            }
        }
        TypeExpression::Union { items, .. } => {
            for item in items {
                validate_type_expression(item)?;
            }
        }
        TypeExpression::Intersection { items, .. } => {
            for item in items {
                validate_type_expression(item)?;
            }
        }
        TypeExpression::Associated { .. } | TypeExpression::Nullable { .. } | TypeExpression::Function { .. } => {}
    }
    Ok(())
}

/// Reject source-level text aliases that do not identify a language encoding.
///
/// This is deliberately a source boundary check. Canonical Semantic MIR has
/// only explicit `Utf8` and `Utf16` types, so an unqualified text name must
/// never reach MIR where a backend carrier could give it accidental meaning.
pub fn validate_source_text_type_name(name: &str) -> Result<(), ParseError> {
    if is_legacy_text_type_name(name) {
        return Err(ParseError::invalid(format!(
            "ambiguous text type `{name}` is forbidden; use an explicit encoding such as `utf8`, `utf16`, or `c_str`"
        )));
    }
    Ok(())
}

/// 将 `AST` 类型表达式降到最小 `HIR` 类型表示。
pub(crate) fn lower_type_expression(ty: &TypeExpression) -> ValkyrieType {
    match ty {
        TypeExpression::Path(path) => lower_type_path(path),
        TypeExpression::Array { item, .. } => ValkyrieType::Array(Box::new(lower_type_expression(item))),
        TypeExpression::FixedArray { item, length, .. } => {
            ValkyrieType::FixedArray { element: Box::new(lower_type_expression(item)), length: usize::try_from(*length).unwrap_or(usize::MAX) }
        }
        TypeExpression::Tuple { items, .. } => {
            if items.is_empty() {
                ValkyrieType::Unit
            }
            else {
                ValkyrieType::Tuple(items.iter().map(lower_type_expression).collect())
            }
        }
        TypeExpression::Pointer { item, .. } => lower_type_expression(item),
        TypeExpression::Row { methods, .. } => ValkyrieType::Row(RowType {
            methods: methods
                .iter()
                .map(|method| RowMethodType {
                    name: method.name.name.clone(),
                    params: method.params.iter().map(lower_type_expression).collect(),
                    return_type: lower_type_expression(&method.return_type),
                })
                .collect(),
        }),
        TypeExpression::Associated { ty, .. } => lower_type_expression(ty),
        TypeExpression::Nullable { item, .. } => flatten_nullable_type(lower_type_expression(item)),
        TypeExpression::Function { params, return_type, .. } => ValkyrieType::Function(Box::new(FunctionType {
            params: params.iter().map(lower_type_expression).collect(),
            return_type: lower_type_expression(return_type),
        })),
        TypeExpression::Union { items, .. } => ValkyrieType::Union(items.iter().map(lower_type_expression).collect()),
        TypeExpression::Intersection { items, .. } => ValkyrieType::Intersection(items.iter().map(lower_type_expression).collect()),
    }
}

/// `T?` lowers to a structured nullable type; nested `T??` flattens to one layer.
fn flatten_nullable_type(inner: ValkyrieType) -> ValkyrieType {
    match inner {
        ValkyrieType::Nullable(payload) => ValkyrieType::Nullable(payload),
        payload => ValkyrieType::Nullable(Box::new(payload)),
    }
}

/// 渲染类型表达式，供错误消息与回退路径使用。
pub(crate) fn render_type_expression(ty: &TypeExpression) -> String {
    match ty {
        TypeExpression::Path(path) => {
            let base = path.name.parts.join("::");
            if path.arguments.is_empty() {
                base
            }
            else {
                let args = path.arguments.iter().map(render_type_expression).collect::<Vec<_>>().join(", ");
                format!("{base}<{args}>")
            }
        }
        TypeExpression::Array { item, .. } => format!("[{}]", render_type_expression(item)),
        TypeExpression::FixedArray { item, length, .. } => format!("[{}; {length}]", render_type_expression(item)),
        TypeExpression::Tuple { items, .. } => {
            if items.is_empty() {
                return "()".to_string();
            }
            let inner = items.iter().map(render_type_expression).collect::<Vec<_>>().join(", ");
            format!("({inner})")
        }
        TypeExpression::Pointer { kind, item, .. } => {
            let prefix = match kind {
                std_data::text::valkyrie::ast::PointerKind::ReadOnly => "\u{25C7}",
                std_data::text::valkyrie::ast::PointerKind::Mutable => "\u{25C6}",
            };
            format!("{prefix}{}", render_type_expression(item))
        }
        TypeExpression::Row { methods, .. } => {
            let inner = methods
                .iter()
                .map(|method| {
                    let params = method.params.iter().map(render_type_expression).collect::<Vec<_>>().join(", ");
                    format!("{}({params}) -> {}", method.name, render_type_expression(&method.return_type))
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{ {inner} }}")
        }
        TypeExpression::Associated { name, ty, .. } => format!("{name}={}", render_type_expression(ty)),
        TypeExpression::Nullable { item, .. } => format!("{}?", render_type_expression(item)),
        TypeExpression::Union { items, .. } => items.iter().map(render_type_expression).collect::<Vec<_>>().join(" | "),
        TypeExpression::Intersection { items, .. } => items.iter().map(render_type_expression).collect::<Vec<_>>().join(" & "),
        TypeExpression::Function { params, return_type, .. } => {
            let params_str = params.iter().map(render_type_expression).collect::<Vec<_>>().join(", ");
            format!("micro({params_str}) -> {}", render_type_expression(return_type))
        }
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
        // C ABI / syscall 字节串：显式身份，禁止与 utf8 混用弱别名 `string`
        "c_str" => Some(ValkyrieType::Named(Identifier::new("c_str"))),
        "unit" => Some(ValkyrieType::Unit),
        "void" => Some(ValkyrieType::Void),
        _ => None,
    }
}

fn collect_shadowed_builtin_type_aliases(root: &ValkyrieRoot) -> BTreeSet<String> {
    root.statements
        .iter()
        .filter_map(|statement| match statement {
            RootStatement::TypeAlias(alias) if canonical_builtin_type(alias.name.name.as_str()).is_some() => {
                Some(alias.name.name.as_str().to_string())
            }
            _ => None,
        })
        .collect()
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
            // 形参个数不匹配时不展开，留给后续类型检查报错。
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

fn lower_type_path(path: &AstTypePath) -> ValkyrieType {
    let last = path.name.parts.last().cloned().unwrap_or_default();
    if path.name.parts.len() == 1 && last.as_str() == "Self" {
        return ValkyrieType::SelfType;
    }
    let base = if !is_shadowed_builtin_type(last.as_str())
        && (path.name.parts.len() == 1 || is_known_builtin_type_namespace(&path.name.parts, last.as_str()))
    {
        canonical_builtin_type(last.as_str()).unwrap_or_else(|| ValkyrieType::Named(Identifier::new(&last)))
    }
    else {
        ValkyrieType::Named(Identifier::new(&last))
    };
    let arguments: Vec<ValkyrieType> = path.arguments.iter().map(lower_type_expression).collect();
    if path.name.parts.len() == 1 {
        if let Some(expanded) = expand_module_type_alias(last.as_str(), &arguments, &mut BTreeSet::new()) {
            return expanded;
        }
    }
    if arguments.is_empty() { base } else { ValkyrieType::Apply(Box::new(base), arguments) }
}

fn is_known_builtin_type_namespace(parts: &[String], last: &str) -> bool {
    let prefix = &parts[..parts.len().saturating_sub(1)];
    match prefix {
        [core, primitive] if core == "core" && primitive == "primitive" => canonical_builtin_type(last).is_some(),
        [core, text] if core == "core" && text == "text" => matches!(last, "char" | "utf8" | "utf16" | "c_str"),
        [std, text] if std == "std" && text == "text" => matches!(last, "utf8" | "utf16" | "c_str"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std_data::text::valkyrie::ast::TypePath;

    fn path_type(name: &str) -> TypeExpression {
        TypeExpression::Path(TypePath {
            name: std_data::text::valkyrie::ast::NamePath { parts: vec![name.to_string()], span: 0..name.len() },
            arguments: Vec::new(),
            span: 0..name.len(),
        })
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
        assert_eq!(canonical_builtin_type("c_str"), Some(ValkyrieType::Named(Identifier::new("c_str"))));
        assert!(canonical_builtin_type("string").is_none());
    }
}
