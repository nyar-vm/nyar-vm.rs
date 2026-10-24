//! Bash adapter 合同：module / semantic bridge / interpreter 不依赖 Valkyrie IR。

use std::collections::HashMap;

use rusty_bash::{BashModule, BashScript, BashSemanticBridge, evaluate_bash_source};

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
