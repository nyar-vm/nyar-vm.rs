use rusty_tcl::*;
use std::collections::HashMap;

#[test]
fn evaluates_puts_set() {
    let mut env = HashMap::new();
    let result = evaluate_tcl_source("set x 1\nputs $x", &mut env).expect("eval");
    assert_eq!(result.to_string_value(), "1");
}

#[test]
fn evaluates_proc_foreach() {
    let source = r#"
proc add {a b} {
  return [expr {$a + $b}]
}
set total 0
foreach x {10 20} {
  set total [add $total $x]
}
puts $total
"#;
    let mut env = HashMap::new();
    let result = evaluate_tcl_source(source, &mut env).expect("eval");
    assert_eq!(result.to_string_value(), "30");
}

#[test]
fn evaluates_for_and_braced_subst() {
    let source = r#"
set name victory
proc greet {who} {
  return $who
}
set x 0
while {$x < 1} {
  incr x
}
if {$x == 1} {
  for {set i 0} {$i < 1} {incr i} {
    puts [greet ${name}]
  }
}
"#;
    let mut env = HashMap::new();
    let result = evaluate_tcl_source(source, &mut env).expect("eval");
    assert_eq!(result.to_string_value(), "victory");
}
