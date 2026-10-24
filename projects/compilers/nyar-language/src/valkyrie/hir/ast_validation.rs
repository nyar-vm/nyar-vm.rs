//! `AST -> HIR` 前的结构与语义预校验。

use crate::valkyrie::{
    frontend::{
        ValkyrieRoot,
        ast::{EnumsKind, NamePath, StatementNode},
    },
    hir::HirError,
};
use std::collections::BTreeMap;

use nyar_types::{DiagnosticSource, DiagnosticSpan};

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
                if let Some((name, kind, span)) = type_declaration_identity(other) {
                    let key = (current_namespace.clone(), name);
                    if let Some(prior) = seen.insert(key.clone(), kind) {
                        return Err(HirError::DuplicateTypeDefinition {
                            name: key.1.clone(),
                            namespace: namespace_display(&key.0),
                            prior_kind: prior.to_string(),
                            current_kind: kind.to_string(),
                            source: DiagnosticSource { id: "<ast>".into(), revision: None },
                            span: DiagnosticSpan { start: span.start, end: span.end },
                        });
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

fn type_declaration_identity(item: &StatementNode) -> Option<(String, &'static str, oak_core::Range<usize>)> {
    match item {
        StatementNode::Class(item) => Some((item.name.name.clone(), "class", item.span)),
        StatementNode::Structure(item) => Some((item.name.name.clone(), "structure", item.span)),
        StatementNode::Union(item) => Some((item.name.name.clone(), "union", item.span)),
        StatementNode::Singleton(item) => Some((item.name.name.clone(), "singleton", item.span)),
        StatementNode::Flags(item) => Some((item.name.name.clone(), "flags", item.span)),
        StatementNode::Enums(item) => {
            let kind = match item.kind {
                EnumsKind::Unity => "unite",
                EnumsKind::Enums | EnumsKind::Enum => "enums",
            };
            Some((item.name.name.clone(), kind, item.span))
        }
        StatementNode::Variant(item) => Some((item.name.name.clone(), "unite", item.span)),
        StatementNode::Trait(item) => Some((item.name.name.clone(), "trait", item.span)),
        StatementNode::Widget(item) => Some((item.name.name.clone(), "widget", item.span)),
        StatementNode::TypeAlias(item) => Some((item.name.name.clone(), "type alias", item.span)),
        _ => None,
    }
}
