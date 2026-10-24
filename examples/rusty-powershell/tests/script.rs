use rusty_powershell::script::PowerShellScript;

#[test]
fn parse_fixture_line() {
    let script = PowerShellScript::parse(r#"Write-Output "legend powershell fixture""#).expect("parse");
    assert_eq!(script.statements.len(), 1);
}
