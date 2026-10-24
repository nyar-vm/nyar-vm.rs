//! Migrated from `src/awsl/expr_deps.rs` (`tests`).

use super::*;

#[test]
fn extracts_idents_from_expr() {
    let idents = extract_expr_idents("filter == 'all' ? 'active' : ''");
    assert!(idents.contains(&"filter".to_string()));
}

#[test]
fn reactive_deps_filters_mut_bindings() {
    let bindings = vec![
        ScriptBinding {
            name: "filter".into(),
            init_expr: "\"all\"".into(),
            kind: super::super::lower::BindingKind::ReactiveState,
            reactive: true,
            sig_var: "filter_sig".into(),
            value_type: super::super::lower::SignalValueType::Utf8,
        },
        ScriptBinding {
            name: "label".into(),
            init_expr: "\"x\"".into(),
            kind: super::super::lower::BindingKind::LocalConst,
            reactive: false,
            sig_var: String::new(),
            value_type: super::super::lower::SignalValueType::Utf8,
        },
    ];
    let deps = reactive_deps("filter == 'all'", &bindings);
    assert_eq!(deps, vec!["filter"]);
}
