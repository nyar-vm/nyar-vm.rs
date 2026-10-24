//! Bash language smoke tests.

use std::{collections::HashMap, fs, path::PathBuf};

use legacy_vm::LegacyVmRunner;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../legend/fixtures/bash").join(name);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[test]
fn bash_echo_and_if() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let source = r#"
x=1
if [ $x -eq 1 ]; then
  echo hello
fi
"#;
    let result = runner.run("bash", source, &mut env).expect("run");
    assert_eq!(result.to_string_value(), "hello");
}

#[test]
fn bash_while_for_function_printf() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let source = r#"
greet() {
  printf "%s" "$1"
}
x=0
while [ $x -lt 1 ]; do
  x=1
done
for item in hi; do
  greet $item
done
"#;
    let result = runner.run("bash", source, &mut env).expect("run");
    assert_eq!(result.to_string_value(), "hi");
}

#[test]
fn bash_exit_code_via_status() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let result = runner.run("bash", "false\necho $?", &mut env).expect("run");
    assert_eq!(result.to_string_value(), "1");
}

#[test]
fn bash_exit_builtin() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let result = runner.run("bash", "echo before\nexit 3\necho after", &mut env).expect("run");
    assert_eq!(result.to_i64(), 3);
}

#[test]
fn bash_if_else() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let result = runner
        .run(
            "bash",
            "x=0
if [ $x -eq 1 ]; then
  echo then
else
  echo else
fi",
            &mut env,
        )
        .expect("run");
    assert_eq!(result.to_string_value(), "else");
}

#[test]
fn bash_and_or() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let result = runner.run("bash", "false || echo fallback", &mut env).expect("run");
    assert_eq!(result.to_string_value(), "fallback");
}

#[test]
fn bash_legend_hello_fixture() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let result = runner.run("bash", &fixture("hello.sh"), &mut env).expect("run");
    assert_eq!(result.to_string_value(), "legend bash fixture");
}

#[test]
fn bash_legend_control_fixture() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let result = runner.run("bash", &fixture("control.sh"), &mut env).expect("run");
    assert_eq!(result.to_string_value(), "victory");
}

#[test]
fn detects_bash_from_extension() {
    let language = LegacyVmRunner::detect_language_from_file("demo.sh", "echo hi");
    assert_eq!(language.as_deref(), Some("bash"));
}

#[test]
fn detects_bash_from_shebang() {
    let language = LegacyVmRunner::detect_language_from_file("demo", "#!/usr/bin/env bash\necho hi");
    assert_eq!(language.as_deref(), Some("bash"));
}

#[test]
fn sh_alias_runs_bash_evaluator() {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let result = runner.run("sh", "echo alias", &mut env).expect("run");
    assert_eq!(result.to_string_value(), "alias");
}
