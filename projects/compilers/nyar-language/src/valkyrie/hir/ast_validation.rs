//! `AST -> HIR` 前的结构与语义预校验。

use crate::valkyrie::{
    hir::HirError,
    frontend::{ValkyrieRoot,
        ast::{EnumsKind, NamePath, StatementNode},
    },
};
use std::collections::BTreeMap;

/// 校验 Oak `AST` 根节点是否满足进入 `HIR` lowering 的前提。
pub(crate) fn validate_ast_root(root: &ValkyrieRoot) -> Result<(), HirError> {
    validate_duplicate_type_names(&root.items, &[])
}

/// 重复定义键必须带当前 namespace 前缀。
///
/// `namespace first;` 是空 items 指令，后续顶层声明落在该作用域；
/// 不得只按短名判重，否则跨 namespace 同名结构会被误杀。
fn validate_duplicate_type_names(items: &[StatementNode], namespace_prefix: &[String]) -> Result<(), HirError> {
    let mut seen: BTreeMap<(Vec<String>, String), &'static str> = BTreeMap::new();
    let mut current_namespace = namespace_prefix.to_vec();
    for item in items {
        match item {
            StatementNode::Namespace(namespace) if !namespace.has_body() => {
                current_namespace = name_path_parts(&namespace.name);
            }
            StatementNode::Namespace(namespace) => {
                if let Some(body) = &namespace.body {
                    let nested = append_namespace_path(&current_namespace, &name_path_parts(&namespace.name));
                    validate_duplicate_type_names(&body.items, &nested)?;
                }
            }
            other => {
                if let Some((name, kind)) = type_declaration_identity(other) {
                    let key = (current_namespace.clone(), name);
                    if let Some(prior) = seen.insert(key.clone(), kind) {
                        return Err(HirError::invalid(format!(
                            "duplicate definition of `{}` in namespace `{}`: {prior} and {kind}",
                            key.1,
                            namespace_display(&key.0)
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

fn name_path_parts(path: &NamePath) -> Vec<String> {
    path.parts.iter().map(|part| part.name.clone()).collect()
}

fn append_namespace_path(prefix: &[String], segment: &[String]) -> Vec<String> {
    let mut path = prefix.to_vec();
    path.extend_from_slice(segment);
    path
}

fn namespace_display(path: &[String]) -> String {
    path.join(".")
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
