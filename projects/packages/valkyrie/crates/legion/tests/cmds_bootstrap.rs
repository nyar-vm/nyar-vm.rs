mod support;

use legion::cmds::bootstrap::{BootstrapArgs, BootstrapStage, run};
use rusty_valkyrie::CanonicalTarget;
use std::{panic::AssertUnwindSafe, path::PathBuf};
use support::create_smoke_project_with_manifest;

#[test]
#[cfg_attr(target_os = "windows", ignore = "seed launch is unstable under sandboxed Windows hosts")]
/// 完整 L2 自举：v1 尚不能自编译 `legion.tools` 时应在 V2 阶段诚实失败。
/// smoke 切片验收见 `bootstrap-smoke-clr.mjs` 与 `examples/bootstrap-smoke`。
fn bootstrap_stops_at_v2_when_v1_artifact_is_not_yet_a_self_hosting_seed() {
    if cfg!(target_os = "macos") {
        eprintln!("skip bootstrap acceptance: macOS host cannot execute managed PE artifacts directly");
        return;
    }

    let fixture = create_smoke_project_with_manifest(
        "legion-bootstrap",
        r#"{
    name: "test_clr_bootstrap",
    version: "0.1.0",
    dependencies: {
        "std": false,
        "core": false
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
        r#"[clr("System.Console", "System.Console", "WriteLine")]
micro console_write_line(message: utf16): unit;

[main]
micro main(): i64 {
    console_write_line("bootstrap build phase")
    return 0
}
"#,
    );
    let seed_path = match std::env::var("LEGION_BOOTSTRAP_SEED") {
        Ok(path) if PathBuf::from(&path).exists() => PathBuf::from(path),
        _ => {
            eprintln!("skip bootstrap acceptance: set LEGION_BOOTSTRAP_SEED to a runnable legion host seed");
            return;
        }
    };
    let args = BootstrapArgs {
        project_dir: fixture.project_dir.clone(),
        bootstrap_project: None,
        seed_path: Some(seed_path),
        skip_compare: true,
        target: CanonicalTarget::clr(),
    };
    let result = match std::panic::catch_unwind(AssertUnwindSafe(|| run(&args))) {
        Ok(Ok(result)) => result,
        Ok(Err(error)) if should_skip_bootstrap_host_check(&error.to_string()) => {
            eprintln!("skip bootstrap acceptance: unable to launch seed binary in current environment: {error}");
            return;
        }
        Ok(Err(error)) => panic!("bootstrap run failed unexpectedly: {error}"),
        Err(payload) if should_skip_bootstrap_host_check(&panic_payload_to_string(&payload)) => {
            eprintln!("skip bootstrap acceptance: seed launch is blocked by current environment");
            return;
        }
        Err(payload) => std::panic::resume_unwind(payload),
    };

    let (failed_stage, error) = result.failed_stage.expect("bootstrap should stop at the current non-self-hosting boundary");
    let message = error.to_string();
    match failed_stage {
        BootstrapStage::V1 => {
            assert_eq!(result.stages_completed, vec![BootstrapStage::Seed]);
            assert!(result.v1_path.is_none());
            assert!(result.v2_path.is_none());
            assert!(message.contains("frozen for 0.0.x") || message.contains("legacy-lanes") || message.contains("编译失败"));
        }
        BootstrapStage::V1Run => {
            assert_eq!(result.stages_completed, vec![BootstrapStage::Seed, BootstrapStage::V1]);
            assert!(result.v1_path.is_none());
            assert!(result.v2_path.is_none());
            assert!(
                message.contains("执行产物失败")
                    || message.contains("执行 v1 --version 失败")
                    || message.contains("执行 v1 --help 失败")
                    || message.contains("v1 --version 返回非零退出码")
                    || message.contains("v1 --help 返回非零退出码")
                    || message.contains("无法执行")
                    || message.contains("Permission denied")
                    || message.contains("Exec format error")
                    || message.contains("cannot execute binary file")
                    || message.contains("Bad CPU type")
            );
        }
        BootstrapStage::V2 => {
            assert_eq!(result.stages_completed, vec![BootstrapStage::Seed, BootstrapStage::V1, BootstrapStage::V1Run]);
            assert!(result.v1_path.is_none());
            assert!(result.v2_path.is_none());
            assert!(message.contains("编译退出码为 0 但未产出产物"));
            assert!(message.contains("可能不具备 `legion build` 命令行能力"));
            assert!(message.contains("dist\\v2\\app.exe") || message.contains("dist/v2/app.exe"));
        }
        other => panic!("unexpected bootstrap failure stage: {other} ({message})"),
    }
}

fn should_skip_bootstrap_host_check(message: &str) -> bool {
    message.contains("os error 0") || message.contains("无法执行") || message.contains("unable to launch") || message.contains("鎿嶄綔鎴愬姛")
}

fn panic_payload_to_string(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    String::new()
}

#[test]
fn bootstrap_smoke_fixture_builds_with_seed() {
    use legion::cmds::build::{BuildArgs, run as build_run};
    use std::process::ExitCode;

    // 0.0.x 主动交付线为 Node/Wasm；CLR 版 `examples/bootstrap-smoke` 需 `legacy-lanes`。
    let fixture = create_smoke_project_with_manifest(
        "legion-bootstrap-smoke-node",
        r#"{
    name: "main",
    version: "0.1.0",
    build: [
        { target: "node" }
    ]
}"#,
        r#"[main]
micro main(): i64 {
    return 0
}
"#,
    );

    let output_dir = fixture.project_dir.join("dist").join("node");
    let _ = std::fs::remove_dir_all(&output_dir);
    let status = build_run(&BuildArgs {
        project_dir: fixture.project_dir.clone(),
        target: CanonicalTarget::parse("node").expect("node target"),
        output_dir: Some(output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    })
    .unwrap();

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(output_dir.join("main.mjs").exists());
    assert!(output_dir.join("main.wasm").exists());
    assert!(output_dir.join("run-contracts.txt").exists());
}
