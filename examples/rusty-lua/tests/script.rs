use rusty_lua::script::{LuaError, LuaScript};

#[test]
fn parse_minimal_script() {
    let script = LuaScript::parse("print('hello')").expect("parse");
    assert_eq!(script.statements.len(), 1);
}

#[test]
fn rejects_empty() {
    assert_eq!(LuaScript::parse("   ").unwrap_err(), LuaError::EmptyScript);
}
