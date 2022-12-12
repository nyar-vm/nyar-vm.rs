use nyar_language::module::*;
use nyar_types::{Identifier, QualifiedName};
use std::path::PathBuf;

fn qn(s: &str) -> QualifiedName {
    QualifiedName::new(s.split("::").map(Identifier::new).collect())
}

#[test]
fn test_qualified_name_display() {
    let name = qn("std::collections::HashMap");
    assert_eq!(format!("{}", name), "std::collections::HashMap");
}

#[test]
fn test_module_error_display() {
    let name = qn("missing::module");
    let error = ModuleError::NotFound { name, searched: vec![PathBuf::from("/path/a"), PathBuf::from("/path/b")] };
    let message = format!("{}", error);
    assert!(message.contains("missing::module"));
    assert!(message.contains("/path/a"));
}
