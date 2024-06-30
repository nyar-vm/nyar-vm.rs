//! `AST -> HIR` 前的结构与语义预校验。

use crate::valkyrie::frontend::{
    ParseError, ValkyrieRoot,
    ast::{EnumsKind, NamePath, StatementNode},
};
use std::collections::BTreeMap;

/// 校验 Oak `AST` 根节点是否满足进入 `HIR` lowering 的前提。
pub(crate) fn validate_ast_root(root: &ValkyrieRoot) -> Result<(), ParseError> {
    validate_duplicate_type_names(&root.items, "")
}

/// 重复定义键必须带当前 namespace 前缀。
///
/// `namespace first;` 是空 items 指令，后续顶层声明落在该作用域；
/// 不得只按短名判重，否则跨 namespace 同名结构会被误杀。
fn validate_duplicate_type_names(items: &[StatementNode], namespace_prefix: &str) -> Result<(), ParseError> {
    let mut seen: BTreeMap<(String, String), &'static str> = BTreeMap::new();
    let mut current_namespace = namespace_prefix.to_string();
    for item in items {
        match item {
            StatementNode::Namespace(namespace) if !namespace.has_body() => {
                current_namespace = name_path_text(&namespace.name);
            }
            StatementNode::Namespace(namespace) => {
                if let Some(body) = &namespace.body {
                    let nested = join_namespace(&current_namespace, &name_path_text(&namespace.name));
                    validate_duplicate_type_names(&body.items, &nested)?;
                }
            }
            other => {
                if let Some((name, kind)) = type_declaration_identity(other) {
                    let key = (current_namespace.clone(), name);
                    if let Some(prior) = seen.insert(key.clone(), kind) {
                        return Err(ParseError::invalid(format!(
                            "duplicate definition of `{}` in namespace `{}`: {prior} and {kind}",
                            key.1,
                            key.0
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

fn name_path_text(path: &NamePath) -> String {
    path.parts.iter().map(|part| part.name.as_str()).collect::<Vec<_>>().join(".")
}

fn join_namespace(namespace: &str, name: &str) -> String {
    if namespace.is_empty() { name.to_string() } else { format!("{namespace}.{name}") }
}

fn type_declaration_identity(item: &StatementNode) -> Option<(String, &'static str)> {
    match item {
        StatementNode::Class(item) => Some((item.name.name.clone(), "class")),
        StatementNode::Structure(item) => Some((item.name.name.clone(), "structure")),
        StatementNode::Union(item) => Some((item.name.name.clone(), "union")),
        StatementNode::Singleton(item) => Some((item.name.name.clone(), "singleton")),
        StatementNode::Flags(item) => Some((item.name.name.clone(), "flags")),
        StatementNode::Enums(item) => {
            let kind = match item.kind {
                EnumsKind::Unity => "unite",
                EnumsKind::Enums | EnumsKind::Enum => "enums",
            };
            Some((item.name.name.clone(), kind))
        }
        StatementNode::Variant(item) => Some((item.name.name.clone(), "unite")),
        StatementNode::Trait(item) => Some((item.name.name.clone(), "trait")),
        StatementNode::Widget(item) => Some((item.name.name.clone(), "widget")),
        StatementNode::TypeAlias(item) => Some((item.name.name.clone(), "type alias")),
        _ => None,
    }
}
