//! Migrated from `src/cmds/test_engine/targets.rs` (`tests`).

use super::*;

#[test]
fn defaults_to_legion_label() {
    assert_eq!(resolve_test_targets(None), vec!["legion"]);
}

#[test]
fn expands_all() {
    assert_eq!(resolve_test_targets(Some("all")), vec!["legion", "clr", "jvm", "node"]);
}

#[test]
fn splits_list() {
    assert_eq!(resolve_test_targets(Some("legion,clr")), vec!["legion", "clr"]);
    assert_eq!(resolve_test_targets(Some("ovm,node")), vec!["ovm", "node"]);
}

#[test]
fn legion_alias_parses_to_ovm_target() {
    let target = parse_target_label("legion").expect("legion");
    assert_eq!(target.to_string(), "ovm-unknown-unknown-managed");
}

#[test]
fn legacy_nyar_aliases_are_rejected() {
    for label in ["nyar", "nyar-vm", "nyarvm", "nvm"] {
        assert!(parse_target_label(label).is_err(), "legacy alias `{label}` must not resolve");
    }
}
