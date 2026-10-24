//! Tcl adapter 合同：module / semantic bridge / interpreter 不依赖 Valkyrie IR。

use rusty_tcl::{TclModule, TclScript, TclSemanticBridge, evaluate_tcl_source};
use std::collections::HashMap;

#[test]
fn tcl_adapter_builds_from_script_without_mir() {
    let script = TclScript::parse("puts hello").expect("parse tcl");
    let module = TclModule::new(script.clone());
    let bridge = TclSemanticBridge::from_script(script);
    assert_eq!(module.language_id(), "tcl");
    assert!(module.script.commands.len() == 1);
    assert!(bridge.exported_symbols().is_empty());
}

#[test]
fn tcl_interpreter_runs_control_and_subst_demo() {
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
    let result = evaluate_tcl_source(source, &mut env).expect("evaluate tcl");
    assert_eq!(result.to_string_value(), "victory");
}
