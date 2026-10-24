use rusty_lua::*;
use std::collections::HashMap;

#[test]
fn tables_loops_and_concat() {
    let mut env = HashMap::new();
    let source = r#"
local t = {10, 20, name = "lua"}
local sum = 0
for i = 1, #t do
  sum = sum + t[i]
end
local n = 0
repeat
  n = n + 1
until n >= 2
if sum == 30 and n == 2 then
  print(t.name .. ":" .. sum)
end
"#;
    let result = evaluate_lua_source(source, &mut env).expect("eval");
    assert_eq!(result.to_string_value(), "lua:30");
}
