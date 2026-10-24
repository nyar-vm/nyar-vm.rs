//! C adapter 合同：module / semantic bridge / interpreter 不依赖 Valkyrie IR。

use rusty_c::{CModule, CScript, CSemanticBridge, evaluate_c_source};
use std::collections::HashMap;

#[test]
fn c_adapter_builds_from_script_without_mir() {
    let script = CScript::parse("int main(void) { return 0; }").expect("parse c");
    let module = CModule::new(script.clone());
    let bridge = CSemanticBridge::from_script(script);
    assert_eq!(module.language_id(), "c");
    assert_eq!(module.script.items.len(), 1);
    assert_eq!(bridge.exported_symbols(), &["main".to_string()]);
}

#[test]
fn c_interpreter_runs_printf_demo() {
    let source = r#"
#include <stdio.h>
int add(int a, int b) { return a + b; }
int main(void) {
    int x = add(1, 2);
    printf("%d\n", x);
    return 0;
}
    "#;
    let mut env = HashMap::new();
    let result = evaluate_c_source(source, &mut env).expect("evaluate c");
    assert_eq!(result.to_string_value(), "3");
}
