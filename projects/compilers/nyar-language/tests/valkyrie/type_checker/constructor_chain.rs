use nyar_language::{
    type_checker::{ConstructorChainChecker, ConstructorChainError, ConstructorChainErrorKind},
    types::{Identifier, hir::ValkyrieType},
};

#[test]
fn test_checker_creation() {
    let checker = ConstructorChainChecker::new();
    assert!(checker.errors().is_empty());
}

#[test]
fn test_missing_super_call_error() {
    let error = ConstructorChainError::missing_super_call(Identifier::new("Child"), Identifier::new("Parent"), None);
    assert!(error.message.contains("Child"));
    assert!(error.message.contains("Parent"));
    assert!(matches!(error.kind, ConstructorChainErrorKind::MissingSuperCall { .. }));
}

#[test]
fn test_super_call_argument_mismatch_error() {
    let error = ConstructorChainError::super_call_argument_mismatch(Identifier::new("Child"), Identifier::new("Parent"), 2, 1, None);
    assert!(error.message.contains("Expected 2"));
    assert!(error.message.contains("got 1"));
    assert!(matches!(error.kind, ConstructorChainErrorKind::SuperCallArgumentMismatch { .. }));
}

#[test]
fn test_invalid_super_call_order_error() {
    let error = ConstructorChainError::invalid_super_call_order(
        Identifier::new("Child"),
        vec![Identifier::new("ParentA"), Identifier::new("ParentB")],
        vec![Identifier::new("ParentB"), Identifier::new("ParentA")],
        None,
    );
    assert!(error.message.contains("MRO"));
    assert!(matches!(error.kind, ConstructorChainErrorKind::InvalidSuperCallOrder { .. }));
}

#[test]
fn test_duplicate_super_call_error() {
    let error = ConstructorChainError::duplicate_super_call(Identifier::new("Child"), Identifier::new("Parent"), None);
    assert!(error.message.contains("Duplicate"));
    assert!(matches!(error.kind, ConstructorChainErrorKind::DuplicateSuperCall { .. }));
}

#[test]
fn test_invalid_super_call_method_error() {
    let error = ConstructorChainError::invalid_super_call_method(Identifier::new("Child"), Identifier::new("other_method"), None);
    assert!(error.message.contains("other_method"));
    assert!(error.message.contains("initiate"));
    assert!(matches!(error.kind, ConstructorChainErrorKind::InvalidSuperCallMethod { .. }));
}

#[test]
fn test_super_call_argument_type_mismatch_error() {
    let error = ConstructorChainError::super_call_argument_type_mismatch(
        Identifier::new("Child"),
        Identifier::new("Parent"),
        0,
        ValkyrieType::Integer32 { signed: true },
        ValkyrieType::Utf8,
        None,
    );
    assert!(error.message.contains("type mismatch"));
    assert!(matches!(error.kind, ConstructorChainErrorKind::SuperCallArgumentTypeMismatch { .. }));
}
