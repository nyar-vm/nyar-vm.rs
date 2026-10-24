//! Migrated from `src/codegen/expr_eval.rs` (`tests`).

use super::*;

#[test]
fn eval_is_active_helper() {
    let ctx = ExprContext::from_bindings(&[], &json!({ "__current_path": "workflow.html" }));
    assert!(ctx.eval_truthy("isActive('workflow.html')"));
    assert!(!ctx.eval_truthy("isActive('other.html')"));
}

#[test]
fn eval_nested_property_in_loop() {
    let mut ctx = ExprContext::from_bindings(&[], &Value::Null);
    let section = json!({
        "title": "指南",
        "pages": [{ "title": "Intro", "href": "intro.html" }]
    });
    ctx.push_loop("section", "__idx", section, 0);
    let pages = ctx.eval_value("section.pages").as_array().cloned().unwrap();
    assert_eq!(pages.len(), 1);
    ctx.push_loop("page", "__idx", pages[0].clone(), 0);
    assert_eq!(ctx.eval_string("page.href"), "intro.html");
}

#[test]
fn eval_equality_and_ternary() {
    let ctx = ExprContext::from_bindings(&[], &json!({ "active": true, "label": "Go" }));
    assert!(ctx.eval_truthy("active == true"));
    assert_eq!(ctx.eval_string("active ? label : 'none'"), "Go");
    assert_eq!(ctx.eval_string("!active ? 'x' : 'y'"), "y");
}

#[test]
fn eval_string_concat() {
    let ctx = ExprContext::from_bindings(&[], &json!({ "section": { "id": "guides" }, "page": { "id": "intro" } }));
    assert_eq!(ctx.eval_string("'/' + section.id + '/' + page.id"), "/guides/intro");
}

#[test]
fn eval_script_binding_array() {
    let bindings = vec![ScriptBinding {
        name: "items".into(),
        init_expr: "[\"a\", \"b\"]".into(),
        kind: crate::awsl::BindingKind::LocalConst,
        reactive: false,
        sig_var: String::new(),
        value_type: crate::awsl::SignalValueType::Utf8,
    }];
    let ctx = ExprContext::from_bindings(&bindings, &Value::Null);
    let items = ctx.eval_value("items").as_array().cloned().unwrap();
    assert_eq!(items.len(), 2);
}
