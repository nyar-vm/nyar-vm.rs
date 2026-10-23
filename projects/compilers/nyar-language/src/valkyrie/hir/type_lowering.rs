//! Oak `AST` 类型表达式到 `HIR` 类型的 lowering 与预检查。
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

use crate::{
    types::{
        Identifier,
        hir::{FunctionType, GenericType, HirKind, RowMethodType, RowType, ValkyrieType},
    },
    valkyrie::{
        hir::{HirError, type_relation::is_numeric_type_name},
        frontend::{self,
            ast::{
                NamePath as AstNamePath, TypeExpression,
                type_nodes::{FunctionType as AstFunctionType, OptionalType, TupleType},
            },
        },
    },
};
use nyar_types::DiagnosticSpan;
use oak_valkyrie::lexer::token_type::ValkyrieTokenType;

thread_local! {
    static MODULE_TYPE_ALIASES: RefCell<Vec<BTreeMap<String, ModuleTypeAliasEntry>>> = RefCell::new(Vec::new());
}

/// 进入一次 lowering 作用域时可见的单条类型别名。
///
/// `target` 为 `None` 表示该别名声明在本组，目标尚未降低：名字已经可见（因此遮蔽同名内建），
/// 展开则要等目标填入后再进行。可见依赖组导出的别名携带已冻结的目标。
#[derive(Debug, Clone)]
pub(crate) struct VisibleTypeAlias {
    /// 别名短名。
    pub name: String,
    /// 泛型形参名。
    pub params: Vec<String>,
    /// 已冻结的目标类型；本组声明为 `None`。
    pub target: Option<ValkyrieType>,
}

/// 模块类型别名条目：可携带泛型形参；目标未填入时为 `None`。
#[derive(Debug, Clone)]
struct ModuleTypeAliasEntry {
    params: Vec<String>,
    target: Option<ValkyrieType>,
}

/// 管理当前 lowering 过程中的模块 `type` 别名作用域。
///
/// 作用域内容就是「本组（或本根）的 `type` 声明 + 可见依赖导出的别名」，与语义闭包的
/// 可见性合同一致：不可见组的别名不进入作用域，内建遮蔽同样只认作用域内的名字。
#[derive(Debug)]
pub(crate) struct ModuleTypeAliasScope;

impl ModuleTypeAliasScope {
    /// 进入一次 lowering 作用域并注册可见别名。
    ///
    /// 调用方把依赖导出（`target: Some`）与本组待填目标的声明（`target: None`）一起传入；
    /// 本组声明在依赖导出之后插入，故同名时以本组声明为准。
    pub(crate) fn enter(visible: Vec<VisibleTypeAlias>) -> Self {
        let mut aliases = BTreeMap::new();
        for alias in visible {
            aliases.insert(alias.name, ModuleTypeAliasEntry { params: alias.params, target: alias.target });
        }
        MODULE_TYPE_ALIASES.with(|stack| {
            stack.borrow_mut().push(aliases);
        });
        Self
    }

    /// 填入本组别名的目标类型；名字必须已由 [`ModuleTypeAliasScope::enter`] 注册。
    pub(crate) fn register_alias(name: &str, params: Vec<String>, target: ValkyrieType) {
        MODULE_TYPE_ALIASES.with(|stack| {
            if let Some(top) = stack.borrow_mut().last_mut() {
                top.insert(name.to_string(), ModuleTypeAliasEntry { params, target: Some(target) });
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
#[doc(hidden)]
pub fn validate_type_expression(ty: &TypeExpression) -> Result<(), HirError> {
    match ty {
        TypeExpression::Namepath(path) => {
            if let Some(part) = path.parts.last() {
                validate_source_text_type_name(&part.name)?;
                if let Some(canonical_name) = legacy_builtin_type_alias(&part.name) {
                    return Err(HirError::lowering_failed(format!(
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
                validate_type_expression(&element.ty)?;
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
        TypeExpression::Apply(apply) => {
            validate_type_expression(&apply.base)?;
            for argument in &apply.arguments {
                validate_type_expression(argument)?;
            }
        }
    }
    Ok(())
}

/// Reject source-level text aliases that do not identify a language encoding.
pub fn validate_source_text_type_name(name: &str) -> Result<(), HirError> {
    if is_legacy_text_type_name(name) {
        return Err(HirError::lowering_failed(format!(
            "ambiguous text type `{name}` is forbidden; use an explicit encoding such as `utf8`, `utf16`, or `c_str`"
        )));
    }
    Ok(())
}

/// 将 Oak `AST` 类型表达式降到最小 `HIR` 类型表示。
///
/// 无法在当前作用域解析的类型短名会以 `HirError::TypeNotFound` 返回，由调用方
/// 逐层传播；不再静默 fallback 成 `ValkyrieType::Named` 传递到 canonical 阶段。
#[doc(hidden)]
pub fn lower_type_expression(ty: &TypeExpression) -> Result<ValkyrieType, HirError> {
    Ok(match ty {
        TypeExpression::Namepath(path) => lower_type_namepath(path)?,
        TypeExpression::Generic(generic) => ValkyrieType::Generic(GenericType {
            binder: None,
            name: Identifier::new(&generic.name.name),
            kind: HirKind::Type,
            bounds: Vec::new(),
        }),
        TypeExpression::Tuple(tuple) => {
            if tuple.elements.is_empty() {
                ValkyrieType::Unit
            }
            else {
                ValkyrieType::Tuple(
                    tuple.elements.iter().map(|element| lower_type_expression(&element.ty)).collect::<Result<Vec<_>, _>>()?,
                )
            }
        }
        TypeExpression::Function(function) => ValkyrieType::Function(Box::new(FunctionType {
            params: function.params.iter().map(lower_type_expression).collect::<Result<Vec<_>, _>>()?,
            return_type: lower_type_expression(&function.return_type)?,
        })),
        TypeExpression::Optional(optional) => flatten_nullable_type(lower_type_expression(&optional.inner)?),
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
            let base = lower_type_expression(&qualified.ty)?;
            ValkyrieType::Associated(Box::new(crate::types::hir::AssociatedType {
                base,
                name: Identifier::new(&qualified.name.name),
                type_arguments: qualified.trait_path.parts.iter().map(|part| ValkyrieType::Named(Identifier::new(&part.name))).collect(),
            }))
        }
        TypeExpression::Binary(node) => match node.operator {
            ValkyrieTokenType::Pipe => ValkyrieType::Union(vec![lower_type_expression(&node.lhs)?, lower_type_expression(&node.rhs)?]),
            ValkyrieTokenType::Ampersand => {
                ValkyrieType::Intersection(vec![lower_type_expression(&node.lhs)?, lower_type_expression(&node.rhs)?])
            }
            _ => lower_type_expression(&node.lhs)?,
        },
        TypeExpression::Unary(node) => match node.operator {
            ValkyrieTokenType::BracketL => {
                let element = lower_type_expression(&node.base)?;
                if let Some(length_expr) = &node.length {
                    let length = match length_expr {
                        frontend::ast::TermExpression::IntegerLiteral { value, .. } => value.parse().unwrap_or(0),
                        _ => 0,
                    };
                    ValkyrieType::FixedArray { element: Box::new(element), length }
                }
                else {
                    ValkyrieType::Array(Box::new(element))
                }
            }
            _ => lower_type_expression(&node.base)?,
        },
        TypeExpression::Apply(apply) => {
            let base = lower_type_expression(&apply.base)?;
            let arguments = apply.arguments.iter().map(lower_type_expression).collect::<Result<Vec<_>, _>>()?;
            ValkyrieType::Apply(Box::new(base), arguments)
        }
    })
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
            let inner = tuple
                .elements
                .iter()
                .map(|element| match &element.name {
                    Some(name) => format!("{}: {}", name.name, render_type_expression(&element.ty)),
                    None => render_type_expression(&element.ty),
                })
                .collect::<Vec<_>>()
                .join(", ");
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
            ValkyrieTokenType::BracketL => {
                if let Some(length_expr) = &node.length {
                    let length = match length_expr {
                        frontend::ast::TermExpression::IntegerLiteral { value, .. } => value.clone(),
                        _ => "?".to_string(),
                    };
                    format!("[{}; {}]", render_type_expression(&node.base), length)
                }
                else {
                    format!("[{}]", render_type_expression(&node.base))
                }
            }
            _ => render_type_expression(&node.base),
        },
        TypeExpression::Apply(apply) => {
            let args = apply.arguments.iter().map(render_type_expression).collect::<Vec<_>>().join(", ");
            format!("{}<{}>", render_type_expression(&apply.base), args)
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

/// 将内建类型名映射到 `HIR` 类型；未知名字返回 `None`。
///
/// 这里只保留**没有源码声明身份**的编码型内建（文本编码、`bool`、`char`、`unit`/`void`
/// 形态名）；数值原始类型名不是内建：`i8`…`f64` 与 `usize` 一样指 `core/primitive`
/// 的 `structure` 声明身份，由 [`resolve_named_type`] 产出 `Named` 后交给
/// `bind_declared_nominal_types` 冻结成 [`ValkyrieType::Nominal`]。
#[doc(hidden)]
pub fn canonical_builtin_type(name: &str) -> Option<ValkyrieType> {
    match name {
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

/// `i64(x)` / `f64(x)` 等单实参类型名调用：仅数值 builtin，不含 utf8/bool 等（那些走显式转换函数）。
///
/// 这里是**显式转换**的目标形态，与类型名的声明身份无关：源码写出 `i32` 仍指
/// `core/primitive` 的 `structure` 声明，只有转换表达式才需要低层 carrier。
pub(crate) fn numeric_builtin_cast_type(name: &str) -> Option<ValkyrieType> {
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
        _ => None,
    }
}

/// 当前作用域内是否声明了该短名的 `type` 别名（含目标尚未填入的本组声明）。
///
/// 这是内建遮蔽的唯一事实来源：源码声明 `type utf8 = Utf8Text` 之后，`utf8` 就不再是内建编码类型。
fn has_module_type_alias(name: &str) -> bool {
    MODULE_TYPE_ALIASES.with(|stack| stack.borrow().last().is_some_and(|aliases| aliases.contains_key(name)))
}

/// 单段类型名的唯一解析顺序：作用域别名 → 内建类型 → 数值声明引用 → 失败（结构化 TypeNotFound）。
///
/// 别名目标尚未填入（或形参个数不匹配）时保留别名身份为 `ValkyrieType::Named`，
/// 不折算成内建类型：别名是声明身份，不能被同名的内建类型顶替。
///
/// 数值原始类型名（`i8`…`f64`、`usize`、`isize` 与 128 位宽度）属于 `core/primitive`
/// 的 `structure` 声明族：这里只产出待绑定的 `Named`，不按名字生产低层 carrier；
/// 声明身份由 `bind_declared_nominal_types` 冻结，声明不可见时残留 `Named` 由 canonical
/// 以结构化诊断拒绝（CAN003），不会默认成 `Integer32`/`Float64`。
///
/// 无法在作用域内找到别名、也不匹配任何语言内建或数值声明族的名字，以前被错误地 fallback 成
/// `Named` 传递到 canonical 阶段。现在这里直接返回结构化 `TypeNotFound`，由上层
/// 逐层用 `?` 传播到 lowering 主入口。
fn resolve_named_type(name: &str) -> Result<ValkyrieType, HirError> {
    if let Some(expanded) = expand_module_type_alias(name, &[], &mut BTreeSet::new()) {
        return Ok(expanded);
    }
    if has_module_type_alias(name) {
        return Ok(ValkyrieType::Named(Identifier::new(name)));
    }
    if let Some(builtin) = canonical_builtin_type(name) {
        return Ok(builtin);
    }
    if is_numeric_type_name(name) {
        return Ok(ValkyrieType::Named(Identifier::new(name)));
    }
    Err(HirError::TypeNotFound {
        name: name.to_string(),
        source: nyar_types::DiagnosticSource { id: "<type_lowering>".into(), revision: None },
        span: DiagnosticSpan { start: 0, end: 0 },
    })
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
        let target = entry.target?;
        visiting.insert(name.to_string());
        let substituted = if entry.params.is_empty() {
            target
        }
        else {
            let mapping: BTreeMap<String, ValkyrieType> = entry.params.into_iter().zip(arguments.iter().cloned()).collect();
            substitute_named_type_params(&target, &mapping)
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

fn lower_type_namepath(path: &AstNamePath) -> Result<ValkyrieType, HirError> {
    // 只消费 Oak 产出的结构化 NamePath；禁止按 span 切片重解析源码。
    let parts = path.parts.iter().map(|part| part.name.as_str()).collect::<Vec<_>>();
    let last = parts.last().copied().unwrap_or_default();
    if parts.len() == 1 && last == "Self" {
        return Ok(ValkyrieType::SelfType);
    }
    if parts.first().is_some_and(|part| *part == "Self") {
        let mut base = ValkyrieType::SelfType;
        for name in parts.iter().skip(1) {
            base = ValkyrieType::Associated(Box::new(crate::types::hir::AssociatedType {
                base,
                name: Identifier::new(name),
                type_arguments: Vec::new(),
            }));
        }
        return Ok(base);
    }
    // `Type::Assoc`：两段且非已知命名空间前缀时保留 Associated，禁止丢前缀只留短名。
    // `core::…` / `std::…` 等限定路径仍走下方短名/builtin 规则，供 bind 按声明冻结。
    if parts.len() == 2 && !is_known_builtin_type_namespace(&parts, last) && !is_module_root_prefix(parts[0]) {
        let head = parts[0];
        return Ok(ValkyrieType::Associated(Box::new(crate::types::hir::AssociatedType {
            base: resolve_named_type(head)?,
            name: Identifier::new(last),
            type_arguments: Vec::new(),
        })));
    }
    // 停线清理：删除 QualifiedPath 作为语法类型进入 HIR 的路径。多段限定类型名
    // 在 frontend 绑定阶段必须已完全解析为声明身份；无法解析时必须返回前端
    // TypeError，不得带着 QualifiedPath 旁路进入后续 canonical / representation。
    //
    // 当前所有非内建、非 Self、非两段 Associated 的多段名直接按最后一段短名交给
    // resolve_named_type 处理；无法在当前作用域绑定则返回 TypeNotFound，
    // 不再静默 fallback 成 Named。
    Ok(resolve_named_type(last)?)
}

fn is_module_root_prefix(name: &str) -> bool {
    matches!(name, "core" | "std" | "self")
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
