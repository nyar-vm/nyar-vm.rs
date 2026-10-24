//! Migrated from `src/forward.rs` (`tests`).

use super::*;

#[test]
fn builtin_subcommands_are_not_forwarded() {
    let argv = vec![OsString::from("legion"), OsString::from("build")];
    assert!(try_forward_companion(&argv).is_none());
}

#[test]
fn unity_is_delegated_to_legion_unity_companion() {
    assert!(!is_builtin_subcommand("unity"));
}
