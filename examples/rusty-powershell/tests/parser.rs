use rusty_powershell::script::{PowerShellScript, PsExpr, PsStmt};

#[test]
fn parse_write_output() {
    let script = PowerShellScript::parse(r#"Write-Output "hello""#).expect("parse");
    assert!(matches!(
        &script.statements[0],
        PsStmt::Expr(PsExpr::Call { name, .. }) if name.eq_ignore_ascii_case("Write-Output")
    ));
}

#[test]
fn parse_assign_if_while_function() {
    let source = r#"
$x = 1
if ($x -eq 1) { $x = $x + 1 } else { $x = 0 }
while ($x -lt 5) { $x = $x + 1 }
function Add($a, $b) { return $a + $b }
"#;
    let script = PowerShellScript::parse(source).expect("parse");
    assert!(matches!(script.statements[0], PsStmt::Assign { .. }));
    assert!(matches!(script.statements[1], PsStmt::If { .. }));
    assert!(matches!(script.statements[2], PsStmt::While { .. }));
    assert!(matches!(script.statements[3], PsStmt::Function { .. }));
}

#[test]
fn parse_for_and_pipeline() {
    let source = r#"
$sum = 0
for ($i = 1; $i -le 3; $i = $i + 1) { $sum = $sum + $i }
$sum | Write-Output
"#;
    let script = PowerShellScript::parse(source).expect("parse");
    assert!(matches!(script.statements[1], PsStmt::For { .. }));
    assert!(matches!(&script.statements[2], PsStmt::Expr(PsExpr::Pipeline { .. })));
}
