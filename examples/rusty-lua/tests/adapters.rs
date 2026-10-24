//! Lua adapter 合同：module / semantic bridge / interpreter 不依赖 Valkyrie IR。

use rusty_lua::{LuaModule, LuaScript, LuaSemanticBridge, evaluate_lua_source};
use std::collections::HashMap;

#[test]
fn lua_adapter_builds_from_script_without_mir() {
    let script = LuaScript::parse("print('hello')").expect("parse lua");
    let module = LuaModule::new(script.clone());
    let bridge = LuaSemanticBridge::from_script(script);
    assert_eq!(module.language_id(), "lua");
    assert!(module.script.statements.len() == 1);
    assert!(bridge.exported_symbols().is_empty());
}

#[test]
fn lua_interpreter_runs_print_demo() {
    let mut env = HashMap::new();
    let result = evaluate_lua_source("local x = 1 + 2\nprint(x)", &mut env).expect("evaluate lua");
    assert_eq!(result.to_string_value(), "3");
}
