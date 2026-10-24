//! Simple script language adapters must stay independent of Valkyrie IR.

use nyar_language::{
    CModule, CSemanticBridge, PowerShellModule, PowerShellSemanticBridge, evaluate_c_source, evaluate_powershell_source,
    transitional::guest_scripts::{c::CScript, powershell::PowerShellScript},
};
use std::collections::HashMap;

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
