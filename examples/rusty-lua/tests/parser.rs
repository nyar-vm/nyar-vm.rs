use rusty_lua::script::{LuaNode, LuaScript};

#[test]
fn parse_local_and_print() {
    let script = LuaScript::parse("local x = 1 + 2\nprint(x)").expect("parse");
    assert_eq!(script.statements.len(), 2);
}

#[test]
fn parse_if_while() {
    let source = r#"
if x > 0 then
  print("ok")
end
while x < 5 do
  x = x + 1
end
"#;
    let script = LuaScript::parse(source).expect("parse");
    assert!(script.statements.len() >= 2);
}

#[test]
fn parse_repeat_for_tables() {
    let source = r#"
local t = {1, 2, name = "lua"}
local a, b = 1, 2
repeat
  a = a + 1
until a > 2
for i = 1, 3 do
  t[i] = i
end
print(t.name .. #t)
"#;
    let script = LuaScript::parse(source).expect("parse");
    assert!(script.statements.len() >= 5);
}

#[test]
fn parse_multi_assign_and_break() {
    let source = r#"
a, b = 10, 20
while true do
  break
end
"#;
    let script = LuaScript::parse(source).expect("parse");
    assert!(matches!(script.statements[0], LuaNode::Assign { .. }));
    assert!(matches!(script.statements[1], LuaNode::While { .. }));
}
