//! Lua language smoke tests (mid-subset).

use std::collections::HashMap;

use legacy_vm::LegacyVmRunner;

#[test]
fn lua_print_local() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();

    let source = r#"
local x = 1 + 2
print(x)
"#;
    let result = runner.run("lua", source, &mut env).expect("run");
    assert_eq!(result.to_string_value(), "3");
}

#[test]
fn lua_tables_control_flow() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();

    let source = r#"
local t = {1, 2, tag = "ok"}
local a, b = 3, 4
local sum = 0
for i = 1, #t do
  sum = sum + t[i]
end
local n = 0
repeat
  n = n + 1
  if n > 10 then
    break
  end
until n >= 2
function add(x, y)
  return x + y
end
local total = add(sum, a + b)
if total == 10 and t.tag == "ok" then
  print(t.tag .. ":" .. total)
end
"#;
    let result = runner.run("lua", source, &mut env).expect("run");
    assert_eq!(result.to_string_value(), "ok:10");
}

#[test]
fn detects_lua_from_extension() {
    let language = LegacyVmRunner::detect_language_from_file("demo.lua", "print(1)");
    assert_eq!(language.as_deref(), Some("lua"));
}
