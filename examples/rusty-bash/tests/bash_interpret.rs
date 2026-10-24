use rusty_bash::*;
use std::collections::HashMap;

#[test]
fn evaluates_vars_if_else_echo() {
    let source = r#"
x=1
if [ $x -eq 1 ]; then
  echo ok
else
  echo no
fi
"#;
    let mut env = HashMap::new();
    let result = evaluate_bash_source(source, &mut env).expect("eval");
    assert_eq!(result.to_string_value(), "ok");
}

#[test]
fn evaluates_loops_function_printf_and_exit() {
    let source = r#"
add() {
  printf "%s" "$1"
}
n=0
while [ $n -lt 1 ]; do
  n=1
done
for item in hi; do
  add $item
done
"#;
    let mut env = HashMap::new();
    let result = evaluate_bash_source(source, &mut env).expect("eval");
    assert_eq!(result.to_string_value(), "hi");
}

#[test]
fn exit_stops_script_with_code() {
    let mut env = HashMap::new();
    let result = evaluate_bash_source("echo before\nexit 7\necho after", &mut env).expect("eval");
    assert_eq!(result.to_i64(), 7);
    assert_eq!(env.get("?").map(BashValue::to_i64), Some(7));
}

#[test]
fn evaluates_elif_and_export() {
    let source = r#"
export x=0
if [ $x -eq 1 ]; then
  echo one
elif [ $x -eq 0 ]; then
  echo zero
else
  echo other
fi
"#;
    let mut env = HashMap::new();
    let result = evaluate_bash_source(source, &mut env).expect("eval");
    assert_eq!(result.to_string_value(), "zero");
    assert_eq!(env.get("x").map(BashValue::to_string_value).as_deref(), Some("0"));
}
