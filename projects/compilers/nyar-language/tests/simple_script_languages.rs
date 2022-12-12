//! Simple script language adapters must stay independent of Valkyrie IR.

use nyar_language::{
    BashModule, BashSemanticBridge, CModule, CSemanticBridge, LuaModule, LuaSemanticBridge, PowerShellModule, PowerShellSemanticBridge,
    TclModule, TclSemanticBridge, evaluate_bash_source, evaluate_c_source, evaluate_lua_source, evaluate_powershell_source,
    evaluate_tcl_source,
};
use std::collections::HashMap;
use std_data::text::{bash::BashScript, c::CScript, lua::LuaScript, powershell::PowerShellScript, tcl::TclScript};

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

#[test]
fn bash_adapter_builds_from_script_without_mir() {
    let script = BashScript::parse("echo hello").expect("parse bash");
    let module = BashModule::new(script.clone());
    let bridge = BashSemanticBridge::from_script(script);
    assert_eq!(module.language_id(), "bash");
    assert!(!module.script.statements.is_empty());
    assert!(bridge.exported_symbols().is_empty());

    let with_fn = BashScript::parse("greet() { echo hi; }\necho ok").expect("parse bash fn");
    let bridge_fn = BashSemanticBridge::from_script(with_fn);
    assert_eq!(bridge_fn.exported_symbols(), &["greet".to_string()]);
}

#[test]
fn bash_interpreter_runs_demo_subset() {
    let source = r#"
name=victory
greet() {
  printf "%s" "$1"
}
x=0
while [ $x -lt 1 ]; do
  x=1
done
if [ $x -eq 1 ]; then
  for item in "$name"; do
    greet $item
  done
else
  echo no
fi
"#;
    let mut env = HashMap::new();
    let result = evaluate_bash_source(source, &mut env).expect("evaluate bash");
    assert_eq!(result.to_string_value(), "victory");
}

#[test]
fn powershell_adapter_builds_from_script_without_mir() {
    let script = PowerShellScript::parse(r#"Write-Output "hello""#).expect("parse powershell");
    let module = PowerShellModule::new(script.clone());
    let bridge = PowerShellSemanticBridge::from_script(script);
    assert_eq!(module.language_id(), "powershell");
    assert!(!module.script.statements.is_empty());
    assert!(bridge.exported_symbols().is_empty());
}

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

#[test]
fn lua_interpreter_runs_print_demo() {
    let mut env = HashMap::new();
    let result = evaluate_lua_source("local x = 1 + 2\nprint(x)", &mut env).expect("evaluate lua");
    assert_eq!(result.to_string_value(), "3");
}

#[test]
fn powershell_interpreter_runs_write_output_demo() {
    let mut env = HashMap::new();
    let result = evaluate_powershell_source(r#"Write-Output "hello""#, &mut env).expect("evaluate powershell");
    assert_eq!(result.to_string_value(), "hello");
}

#[test]
fn powershell_interpreter_runs_vars_control_flow_and_function() {
    let source = r#"
$x = 1
if ($x -eq 1) { $x = $x + 1 } else { $x = 0 }
while ($x -lt 5) { $x = $x + 1 }
function Add($a, $b) { return $a + $b }
Write-Host (Add $x 2)
"#;
    let mut env = HashMap::new();
    let result = evaluate_powershell_source(source, &mut env).expect("evaluate powershell");
    assert_eq!(result.to_string_value(), "7");
    assert_eq!(env.get("x").map(nyar_language::PowerShellValue::to_i64), Some(5));
}
