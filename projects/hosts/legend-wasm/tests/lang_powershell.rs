//! PowerShell language smoke tests.

use std::{collections::HashMap, fs, path::PathBuf};

use legacy_vm::LegacyVmRunner;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../legend/fixtures/powershell").join(name);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[test]
fn powershell_write_output() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let result = runner.run("powershell", r#"Write-Output "hello""#, &mut env).expect("run");
    assert_eq!(result.to_string_value(), "hello");
}

#[test]
fn powershell_variables_if_while_function() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let source = r#"
$x = 1
if ($x -eq 1) { $x = $x + 1 } else { $x = 0 }
while ($x -lt 5) { $x = $x + 1 }
function Add($a, $b) { return $a + $b }
Write-Output (Add $x 2)
"#;
    let result = runner.run("powershell", source, &mut env).expect("run");
    assert_eq!(result.to_string_value(), "7");
}

#[test]
fn powershell_pipeline_and_for() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let source = r#"
$sum = 0
for ($i = 1; $i -le 3; $i = $i + 1) { $sum = $sum + $i }
$sum | Write-Output
"#;
    let result = runner.run("ps1", source, &mut env).expect("run");
    assert_eq!(result.to_string_value(), "6");
}

#[test]
fn powershell_legend_hello_fixture() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let result = runner.run("powershell", &fixture("hello.ps1"), &mut env).expect("run");
    assert_eq!(result.to_string_value(), "legend powershell fixture");
}

#[test]
fn powershell_legend_control_fixture() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let result = runner.run("powershell", &fixture("control.ps1"), &mut env).expect("run");
    assert_eq!(result.to_string_value(), "victory");
}

#[test]
fn detects_powershell_from_extension() {
    let language = LegacyVmRunner::detect_language_from_file("demo.ps1", "Write-Output hello");
    assert_eq!(language.as_deref(), Some("powershell"));
}

#[test]
fn detects_powershell_from_shebang() {
    let language = LegacyVmRunner::detect_language_from_file("demo", "#!/usr/bin/env pwsh\nWrite-Output hi");
    assert_eq!(language.as_deref(), Some("powershell"));
}
