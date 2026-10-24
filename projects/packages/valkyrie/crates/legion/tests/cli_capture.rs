use std::ffi::OsString;

#[test]
fn run_with_argv_captured_handles_version_flag() {
    let (code, stdout, stderr) = legion::cli::run_with_argv_captured(vec![OsString::from("legion"), OsString::from("--version")]);
    assert_eq!(code, 0);
    let combined = format!("{stdout}{stderr}");
    assert!(combined.contains("vcc"), "expected version text, got stdout={stdout:?} stderr={stderr:?}");
}

#[test]
fn run_with_argv_handles_version_flag() {
    let code = legion::cli::run_with_argv(vec![OsString::from("legion"), OsString::from("--version")]);
    assert_eq!(code, 0);
}
