//! WASM Node CLI dispatch helpers (mirrors `clr_cli.rs` for the Node / JS-glue track).
//!
//! The Node build export is wired in `wasm/mir.rs` by exporting `build_from_cli_state`
//! as the wasm `build` entry instead of synthesizing a no-op stub.

use nyar::QualifiedName;

use crate::FragmentSubmission;

pub(crate) fn is_build_from_cli_operation(operation: &QualifiedName) -> bool {
    operation.parts().last().is_some_and(|part| part.as_str() == "build_from_cli_state")
}

pub(crate) fn is_execute_build_operation(operation: &QualifiedName) -> bool {
    operation.parts().last().is_some_and(|part| part.as_str() == "execute_build" || part.as_str() == "execute_build_from_request")
}

pub(crate) fn node_cli_import_fields() -> &'static [&'static str] {
    &["cli_get_project", "cli_get_target", "cli_get_output", "cli_get_verbose"]
}

pub(crate) fn find_build_export_operation<'a>(submission: &'a FragmentSubmission) -> Option<&'a QualifiedName> {
    submission.exported_operations.iter().find(|operation| is_build_from_cli_operation(operation))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_build_from_cli_operation_name() {
        let operation = nyar::QualifiedName::new(vec![nyar::Identifier::new("legion"), nyar::Identifier::new("build_from_cli_state")]);
        assert!(is_build_from_cli_operation(&operation));
    }
}
