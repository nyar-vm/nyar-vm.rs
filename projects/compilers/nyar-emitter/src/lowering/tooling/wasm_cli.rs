//! WASM Node CLI dispatch helpers（对齐 `clr_cli.rs` 的 Node / JS-glue 轨）。
//!
//! Node `build` 导出应绑定已编译的 `build_from_cli_state`，而不是合成空桩。
//! 表面叶名仅允许出现在本模块的播种表；wasm 装配不得再散落 `parts().last() == "…"`。

use nyar::QualifiedName;

use crate::FragmentSubmission;

/// Node / JS-glue 主包上的 CLI 导出别名播种表：`(操作叶名, wasm 导出名)`。
///
/// 优先仍应使用 [`FragmentSubmission::wasm_export_names`]；本表仅覆盖尚未写入 plan 的
/// `version` / `help` / `build` 过渡别名。
pub(crate) fn node_cli_export_seed_aliases() -> &'static [(&'static str, &'static str)] {
    &[("version_text", "version"), ("print_root_help", "help"), ("build_from_cli_state", "build")]
}

/// 操作叶名是否为 `version_text`（合成 `version` 导出回退路径）。
pub(crate) fn is_version_text_operation(operation: &QualifiedName) -> bool {
    operation_leaf_matches(operation, "version_text")
}

/// 操作叶名是否为 `build_from_cli_state`（仅经播种表判定）。
pub(crate) fn is_build_from_cli_operation(operation: &QualifiedName) -> bool {
    operation_leaf_matches(operation, "build_from_cli_state")
}

/// 操作叶名是否为 execute_build 族。
pub(crate) fn is_execute_build_operation(operation: &QualifiedName) -> bool {
    operation_leaf_matches(operation, "execute_build") || operation_leaf_matches(operation, "execute_build_from_request")
}

pub(crate) fn node_cli_import_fields() -> &'static [&'static str] {
    &["cli_get_project", "cli_get_target", "cli_get_output", "cli_get_verbose"]
}

pub(crate) fn find_build_export_operation<'a>(submission: &'a FragmentSubmission) -> Option<&'a QualifiedName> {
    submission.exported_operations.iter().find(|operation| is_build_from_cli_operation(operation))
}

/// 在操作列表中查找叶名匹配项（CLI 播种表专用；禁止在后端散落复制）。
pub(crate) fn find_operation_by_leaf<'a>(operations: &'a [QualifiedName], leaf: &str) -> Option<&'a QualifiedName> {
    operations.iter().find(|operation| operation_leaf_matches(operation, leaf))
}

/// 解析 Node CLI 导出名对应的操作：先 `wasm_export_names`，再播种表叶名。
pub(crate) fn resolve_node_cli_export_operation<'a>(
    submission: &'a FragmentSubmission,
    operations: &'a [QualifiedName],
    export_name: &str,
) -> Option<&'a QualifiedName> {
    if let Some((operation, _)) = submission.wasm_export_names.iter().find(|(_, public)| public.as_str() == export_name) {
        if operations.iter().any(|op| op == operation) {
            return Some(operation);
        }
    }
    let leaf = node_cli_export_seed_aliases().iter().find(|(_, export)| *export == export_name).map(|(leaf, _)| *leaf)?;
    find_operation_by_leaf(operations, leaf)
}

fn operation_leaf_matches(operation: &QualifiedName, leaf: &str) -> bool {
    operation.parts().last().is_some_and(|part| part.as_str() == leaf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_build_from_cli_operation_name() {
        let operation = nyar::QualifiedName::new(vec![nyar::Identifier::new("legion"), nyar::Identifier::new("build_from_cli_state")]);
        assert!(is_build_from_cli_operation(&operation));
    }

    #[test]
    fn seed_aliases_cover_version_help_build() {
        let aliases = node_cli_export_seed_aliases();
        assert!(aliases.iter().any(|(leaf, export)| *leaf == "version_text" && *export == "version"));
        assert!(aliases.iter().any(|(leaf, export)| *leaf == "print_root_help" && *export == "help"));
        assert!(aliases.iter().any(|(leaf, export)| *leaf == "build_from_cli_state" && *export == "build"));
    }
}
