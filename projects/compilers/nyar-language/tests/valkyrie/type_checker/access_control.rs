//! Tests for access control checking module.

use nyar_language::{
    type_checker::{AccessContext, AccessControlChecker, AccessControlError, AccessControlErrorKind},
    types::{
        Identifier, NamePath, SourceSpan,
        hir::{AccessLevel, HirVisibility},
    },
};
use std::collections::HashMap;

fn create_test_identifier(name: &str) -> Identifier {
    Identifier::new(name)
}

fn create_test_namepath(parts: &[&str]) -> NamePath {
    NamePath::new(parts.iter().map(|s| create_test_identifier(s)).collect())
}

#[test]
fn test_access_level_ordering() {
    assert!(AccessLevel::Public > AccessLevel::Protected);
    assert!(AccessLevel::Protected > AccessLevel::Internal);
    assert!(AccessLevel::Internal > AccessLevel::Private);
}

#[test]
fn test_access_level_is_methods() {
    assert!(AccessLevel::Public.is_public());
    assert!(!AccessLevel::Public.is_protected());
    assert!(!AccessLevel::Public.is_internal());
    assert!(!AccessLevel::Public.is_private());

    assert!(!AccessLevel::Protected.is_public());
    assert!(AccessLevel::Protected.is_protected());
    assert!(!AccessLevel::Protected.is_internal());
    assert!(!AccessLevel::Protected.is_private());

    assert!(!AccessLevel::Internal.is_public());
    assert!(!AccessLevel::Internal.is_protected());
    assert!(AccessLevel::Internal.is_internal());
    assert!(!AccessLevel::Internal.is_private());

    assert!(!AccessLevel::Private.is_public());
    assert!(!AccessLevel::Private.is_protected());
    assert!(!AccessLevel::Private.is_internal());
    assert!(AccessLevel::Private.is_private());
}

#[test]
fn test_access_level_as_str() {
    assert_eq!(AccessLevel::Public.as_str(), "public");
    assert_eq!(AccessLevel::Protected.as_str(), "protected");
    assert_eq!(AccessLevel::Internal.as_str(), "internal");
    assert_eq!(AccessLevel::Private.as_str(), "private");
}

#[test]
fn test_hir_visibility_creation() {
    let public = HirVisibility::public();
    assert!(public.is_public());

    let protected = HirVisibility::protected();
    assert!(protected.is_protected());

    let internal = HirVisibility::internal();
    assert!(internal.is_internal());

    let private = HirVisibility::private();
    assert!(private.is_private());
}

#[test]
fn test_access_context_creation() {
    let module_path = create_test_namepath(&["std", "collections"]);

    let ctx = AccessContext::new(module_path.clone());
    assert_eq!(ctx.current_module, module_path);
    assert!(ctx.current_class.is_none());
    assert!(ctx.current_method.is_none());

    let class_name = create_test_identifier("HashMap");
    let ctx = AccessContext::in_class(module_path.clone(), class_name.clone());
    assert_eq!(ctx.current_module, module_path);
    assert_eq!(ctx.current_class, Some(class_name.clone()));
    assert!(ctx.current_method.is_none());

    let method_name = create_test_identifier("insert");
    let ctx = AccessContext::in_method(module_path.clone(), class_name.clone(), method_name.clone());
    assert_eq!(ctx.current_module, module_path);
    assert_eq!(ctx.current_class, Some(class_name));
    assert_eq!(ctx.current_method, Some(method_name));
}

#[test]
fn test_access_control_checker_creation() {
    let checker = AccessControlChecker::new();
    assert!(checker.errors().is_empty());
    assert!(checker.classes().is_empty());
    assert!(checker.inheritance_map().is_empty());
}

#[test]
fn test_private_member_access_error() {
    let error = AccessControlError::private_member_access(
        create_test_identifier("User"),
        create_test_identifier("password"),
        Some(create_test_identifier("ExternalClass")),
        None,
    );

    assert!(matches!(error.kind, AccessControlErrorKind::PrivateMemberAccess { .. }));
    assert!(error.message.contains("private"));
    assert!(error.message.contains("User"));
    assert!(error.message.contains("password"));
    assert!(error.message.contains("ExternalClass"));
}

#[test]
fn test_protected_member_access_error() {
    let error = AccessControlError::protected_member_access(
        create_test_identifier("Base"),
        create_test_identifier("internal_method"),
        create_test_identifier("UnrelatedClass"),
        None,
    );

    assert!(matches!(error.kind, AccessControlErrorKind::ProtectedMemberAccess { .. }));
    assert!(error.message.contains("protected"));
    assert!(error.message.contains("Base"));
    assert!(error.message.contains("internal_method"));
    assert!(error.message.contains("UnrelatedClass"));
}

#[test]
fn test_internal_member_access_error() {
    let error = AccessControlError::internal_member_access(
        create_test_identifier("helper"),
        create_test_namepath(&["internal", "module"]),
        create_test_namepath(&["external", "module"]),
        None,
    );

    assert!(matches!(error.kind, AccessControlErrorKind::InternalMemberAccess { .. }));
    assert!(error.message.contains("internal"));
    assert!(error.message.contains("helper"));
}

#[test]
fn test_readonly_field_write_error() {
    let error = AccessControlError::readonly_field_write(create_test_identifier("User"), create_test_identifier("id"), None);

    assert!(matches!(error.kind, AccessControlErrorKind::ReadonlyFieldWrite { .. }));
    assert!(error.message.contains("readonly"));
    assert!(error.message.contains("User"));
    assert!(error.message.contains("id"));
}

#[test]
fn test_private_constructor_instantiation_error() {
    let error = AccessControlError::private_constructor_instantiation(create_test_identifier("Singleton"), None);

    assert!(matches!(error.kind, AccessControlErrorKind::PrivateConstructorInstantiation { .. }));
    assert!(error.message.contains("Singleton"));
    assert!(error.message.contains("private"));
    assert!(error.message.contains("factory method"));
}

#[test]
fn test_visibility_reduction_error() {
    let error = AccessControlError::visibility_reduction(
        create_test_identifier("Child"),
        create_test_identifier("method"),
        AccessLevel::Public,
        AccessLevel::Private,
        None,
    );

    assert!(matches!(error.kind, AccessControlErrorKind::VisibilityReduction { .. }));
    assert!(error.message.contains("reduced visibility"));
    assert!(error.message.contains("Child"));
    assert!(error.message.contains("method"));
    assert!(error.message.contains("public"));
    assert!(error.message.contains("private"));
}

#[test]
fn test_error_display() {
    let error =
        AccessControlError::private_member_access(create_test_identifier("TestClass"), create_test_identifier("privateField"), None, None);

    let display = format!("{}", error);
    assert!(display.contains("private"));
    assert!(display.contains("TestClass"));
    assert!(display.contains("privateField"));
}
