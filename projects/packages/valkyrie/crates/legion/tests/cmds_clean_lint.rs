mod support;

use legion::cmds::{
    clean::{CleanArgs, run as run_clean},
    lint::{LintArgs, run as run_lint},
};
use std::{fs, process::ExitCode};
use support::{create_smoke_project, create_smoke_project_with_source};

#[test]
fn clean_removes_dist_and_cache() {
    let fixture = create_smoke_project("legion-clean");
    let dist = fixture.project_dir.join("dist");
    let cache = fixture.project_dir.join(".cache");
    fs::create_dir_all(&dist).unwrap();
    fs::create_dir_all(&cache).unwrap();
    fs::write(dist.join("artifact.txt"), "x").unwrap();

    assert_eq!(run_clean(&CleanArgs { project_dir: fixture.project_dir.clone(), workspace: false }).unwrap(), ExitCode::SUCCESS);
    assert!(!dist.exists());
    assert!(!cache.exists());
}

#[test]
fn lint_accepts_valid_source() {
    let fixture = create_smoke_project_with_source(
        "legion-lint",
        r#"micro main() -> i64 {
    return 0;
}
"#,
    );

    assert_eq!(
        run_lint(&LintArgs { project_dir: fixture.project_dir.clone(), workspace: false, format: None, fix: false }).unwrap(),
        ExitCode::SUCCESS
    );
}
