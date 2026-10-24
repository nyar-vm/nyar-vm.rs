use rusty_bash::script::{BashError, BashScript, BashStmt};

#[test]
fn parse_echo() {
    let script = BashScript::parse("echo hello").expect("parse");
    assert_eq!(script.statements.len(), 1);
}

#[test]
fn parse_if_assign() {
    let source = r#"
x=1
if [ $x -eq 1 ]; then
  echo ok
else
  echo no
fi
"#;
    let script = BashScript::parse(source).expect("parse");
    assert!(script.statements.len() >= 2);
    assert!(matches!(script.statements[1], BashStmt::If { .. }));
}

#[test]
fn parse_while_for_function() {
    let source = r#"
greet() {
  printf "%s" "$1"
}
while [ $x -lt 1 ]; do
  x=1
done
for item in a b; do
  greet $item
done
"#;
    let script = BashScript::parse(source).expect("parse");
    assert!(script.statements.iter().any(|stmt| matches!(stmt, BashStmt::FunctionDef { .. })));
    assert!(script.statements.iter().any(|stmt| matches!(stmt, BashStmt::While { .. })));
    assert!(script.statements.iter().any(|stmt| matches!(stmt, BashStmt::For { .. })));
}

#[test]
fn rejects_empty() {
    assert_eq!(BashScript::parse("   ").unwrap_err(), BashError::EmptyScript);
}

#[test]
fn parse_elif_and_and_or() {
    let source = r#"
if false; then
  echo a
elif true; then
  echo b || echo c
fi
"#;
    let script = BashScript::parse(source).expect("parse");
    assert!(matches!(script.statements[0], BashStmt::If { .. }));
}
