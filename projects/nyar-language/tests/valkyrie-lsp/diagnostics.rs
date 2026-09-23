use nyar_language::types::{ValkyrieError, ValkyrieErrorKind};

#[test]
fn test_naming_violation_uses_dedicated_code() {
    use nyar_language::types::{SourceID, SourceSpan};
    let diag = ValkyrieError::naming_violation(0x0301, "fooBar", SourceSpan::new(SourceID::default(), 0, 6));
    assert_eq!(diag.kind.formatted_code(), "E0301");
    assert_eq!(diag.kind.category(), "命名 lint");
    assert_eq!(diag.level, nyar_language::types::ReportKind::Warning);
    assert!(format!("{diag}").contains("fooBar"));
}

#[test]
fn test_error_code_is_bound_to_error_kind() {
    let io_error = ValkyrieErrorKind::IoError { message: "file not found".to_string(), path: Some("test.v".to_string()) };
    let parse_error = ValkyrieErrorKind::ParseError { message: "unexpected token".to_string() };
    let type_error = ValkyrieErrorKind::TypeError { expected: "Int".to_string(), found: "String".to_string() };

    assert_eq!(io_error.code(), 0x0001);
    assert_eq!(io_error.formatted_code(), "E0001");
    assert_eq!(io_error.category(), "I/O 错误");

    assert_eq!(parse_error.code(), 0x0002);
    assert_eq!(parse_error.formatted_code(), "E0002");
    assert_eq!(parse_error.category(), "解析错误");

    assert_eq!(type_error.code(), 0x0003);
    assert_eq!(type_error.formatted_code(), "E0003");
    assert_eq!(type_error.category(), "类型错误");
}

#[test]
fn test_error_instance_uses_kind_code() {
    let io_error = ValkyrieError::io_error("file not found".to_string(), Some("test.v".to_string()));
    let type_error = ValkyrieError::type_error("Int".to_string(), "String".to_string());
    let compile_error = ValkyrieError::compile_error("compile failed".to_string());

    assert_eq!(io_error.code(), io_error.kind.code());
    assert_eq!(type_error.code(), type_error.kind.code());
    assert_eq!(compile_error.code(), compile_error.kind.code());
    assert_eq!(compile_error.kind.formatted_code(), "E2001");
    assert_eq!(compile_error.kind.category(), "编译错误");
}
