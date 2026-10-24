//! PowerShell adapter 合同：module / semantic bridge / interpreter 不依赖 Valkyrie IR。

use rusty_powershell::{PowerShellModule, PowerShellScript, PowerShellSemanticBridge, PowerShellValue, evaluate_powershell_source};
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
    assert_eq!(env.get("x").map(PowerShellValue::to_i64), Some(5));
}
