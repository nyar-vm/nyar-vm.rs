//! Migrated from `src/awsl/expr_util.rs` (`tests`).

use super::*;

#[test]
fn split_ternary_respects_quotes() {
    let (cond, then_arm, else_arm) = split_awsl_ternary("filter == 'all' ? 'flex p-2' : 'hidden'").expect("ternary");
    assert_eq!(cond, "filter == 'all'");
    assert_eq!(then_arm, "'flex p-2'");
    assert_eq!(else_arm, "'hidden'");
}

#[test]
fn collect_from_ternary() {
    let mut tokens = Vec::new();
    collect_utility_tokens_from_expr("ready ? 'flex p-2' : 'hidden'", &mut tokens);
    assert!(tokens.contains(&"flex".to_string()));
    assert!(tokens.contains(&"p-2".to_string()));
    assert!(tokens.contains(&"hidden".to_string()));
}

#[test]
fn collect_static_utilities() {
    let mut tokens = Vec::new();
    collect_utility_tokens_from_expr("flex w-4 h-4", &mut tokens);
    assert_eq!(tokens, vec!["flex", "w-4", "h-4"]);
}
