//! Tcl language smoke tests.

use std::collections::HashMap;

use legacy_vm::LegacyVmRunner;

#[test]
fn tcl_puts_set() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let result = runner.run("tcl", "set x 1\nputs $x", &mut env).expect("run");
    assert_eq!(result.to_string_value(), "1");
}

#[test]
fn tcl_expr_if_while() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let source = "set n 0\nwhile {$n < 3} {\n  incr n\n}\nputs $n\n";
    let result = runner.run("tcl", source, &mut env).expect("run");
    assert_eq!(result.to_string_value(), "3");
}

#[test]
fn tcl_for_foreach_proc_subst() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let source = r#"
proc add {a b} {
  return [expr {$a + $b}]
}
set total 0
for {set i 0} {$i < 3} {incr i} {
  set total [add $total $i]
}
foreach x {10 20} {
  set total [add $total $x]
}
puts $total
"#;
    let result = runner.run("tcl", source, &mut env).expect("run");
    assert_eq!(result.to_string_value(), "33");
}

#[test]
fn tcl_list_helpers() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let source = "set xs [list a b c]\nputs [lindex $xs 1]\n";
    let result = runner.run("tcl", source, &mut env).expect("run");
    assert_eq!(result.to_string_value(), "b");
}

#[test]
fn detects_tcl_from_extension() {
    let language = LegacyVmRunner::detect_language_from_file("demo.tcl", "puts hi");
    assert_eq!(language.as_deref(), Some("tcl"));
}
