mod support;

use legion::{
    CanonicalTarget,
    cmds::{
        build::{BuildArgs, run},
        run::{CompileEvidenceDigest, ExecutionManifest, RunContract},
    },
    parse_von,
};
use std::{
    fs,
    process::{Command, ExitCode},
};
use support::{create_nyar_vm_std_project, create_smoke_project_with_build, valkyrie_v_std_workspace_available};

fn assert_compile_evidence_sidecar_matches_manifest(output_dir: &std::path::Path, manifest: &ExecutionManifest) {
    let sidecar_path = output_dir.join("compile-evidence.txt");
    assert!(sidecar_path.is_file(), "compile-evidence sidecar must exist at {}", sidecar_path.display());
    let stored = manifest.compile_evidence.as_ref().expect("execution manifest compile evidence");
    let source = fs::read_to_string(&sidecar_path).expect("read compile-evidence sidecar");
    let file = parse_von::<rusty_valkyrie::CompilerCompileEvidence>(&source).expect("parse compile-evidence sidecar");
    let digest = CompileEvidenceDigest::from(&file);
    assert_eq!(digest, *stored, "compile-evidence sidecar must match execution manifest");
}

#[test]
fn builds_minimal_nyar_vm_project() {
    let fixture = create_smoke_project_with_build(
        "legion-build-nyar-vm",
        r#"{
            target: "ovm"
        }"#,
        r#"[main]
micro main() -> i64 {
    return 0
}
"#,
    );
    let output_dir = fixture.project_dir.join("dist").join("custom-nyar-vm");
    let status = run(&BuildArgs {
        project_dir: fixture.project_dir.clone(),
        target: CanonicalTarget::parse("ovm").expect("nyar-vm target"),
        output_dir: Some(output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    })
    .unwrap();

    assert_eq!(status, ExitCode::SUCCESS);
    let nyar_artifacts: Vec<_> = fs::read_dir(&output_dir)
        .expect("list nyar-vm output")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("nyar"))
        .collect();
    assert!(!nyar_artifacts.is_empty(), "expected at least one `.nyar` artifact under {}", output_dir.display());
}

#[test]
fn builds_cross_file_nyar_vm_project_and_prefers_main_entry() {
    let fixture = create_smoke_project_with_build(
        "legion-build-nyar-vm-cross-file",
        r#"{
            target: "ovm"
        }"#,
        // 入口注解必须直接落在 micro 声明上；namespace body 内的 `[main]` 归 namespace，
        // 不构成声明入口身份，因此这里使用文件级 namespace。
        r#"namespace app;

[main]
micro main() -> bool {
    return app::identity(true)
}
"#,
    );
    fs::write(
        fixture.project_dir.join("source").join("helper.v"),
        r#"namespace app {
micro identity(x: bool) -> bool {
    return x
}
}
"#,
    )
    .unwrap();

    let output_dir = fixture.project_dir.join("dist").join("cross-file-nyar-vm");
    let status = run(&BuildArgs {
        project_dir: fixture.project_dir.clone(),
        target: CanonicalTarget::parse("ovm").expect("nyar-vm target"),
        output_dir: Some(output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    })
    .unwrap();
    assert_eq!(status, ExitCode::SUCCESS);

    let manifest = ExecutionManifest::read_from_output_dir(&output_dir).expect("read execution manifest").expect("execution manifest present");
    assert_eq!(manifest.schema_version, 0);
    let evidence = manifest.compile_evidence.as_ref().expect("compile evidence in manifest");
    assert!(!evidence.semantic_closure_hash.is_empty());
    assert!(!evidence.canonical_surface_hash.is_empty());
    assert_compile_evidence_sidecar_matches_manifest(&output_dir, &manifest);
    assert_eq!(manifest.identity_schema_version, nyar_types::IDENTITY_SCHEMA_VERSION);
    assert_eq!(manifest.mir_contract_version, nyar_types::MIR_CONTRACT_VERSION);
    assert_eq!(manifest.layout_plan_version, nyar_types::LAYOUT_PLAN_VERSION);
    assert_eq!(manifest.bytecode_format_version, odin_bytecode::BYTECODE_FORMAT_VERSION);
    assert!(
        manifest.run_contracts.iter().any(|contract| {
            contract.logical_entry == "main"
                || contract.logical_entry.rsplit("::").next() == Some("main")
                || contract.logical_entry.rsplit('.').next() == Some("main")
        }),
        "expected main logical entry, got {:?}",
        manifest.run_contracts
    );
}

#[test]
fn builds_option_nyar_vm_project_with_std() {
    if !valkyrie_v_std_workspace_available() {
        eprintln!("skip builds_option_nyar_vm_project_with_std: sibling valkyrie.v core/std missing");
        return;
    }
    let fixture = create_nyar_vm_std_project(
        "legion-build-nyar-vm-option",
        r#"namespace opt.smoke;

micro value_or_zero(o: Option<i64>) -> i64 {
    if o.is_none() {
        return 0
    }
    return o.unwrap()
}

[main]
micro main() -> i64 {
    return value_or_zero(Some(7))
}
"#,
    );
    let output_dir = fixture.project_dir.join("dist").join("option-nyar-vm");
    let status = run(&BuildArgs {
        project_dir: fixture.project_dir.clone(),
        target: CanonicalTarget::parse("ovm").expect("nyar-vm target"),
        output_dir: Some(output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    })
    .unwrap();
    assert_eq!(status, ExitCode::SUCCESS);
    let has_nyar = fs::read_dir(&output_dir)
        .expect("list option output")
        .filter_map(|entry| entry.ok())
        .any(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("nyar"));
    assert!(has_nyar, "expected `.nyar` under {}", output_dir.display());
}

#[test]
fn builds_multiple_artifacts_from_multiple_main_annotations() {
    let fixture = create_smoke_project_with_build(
        "legion-build-multi-main-node",
        r#"{
            target: "node"
        }"#,
        r#"[main]
micro alpha_entry() -> i64 {
    return 0;
}

[main]
micro beta_entry() -> i64 {
    return 0;
}
"#,
    );
    let output_dir = fixture.project_dir.join("dist").join("custom-node");
    let status = run(&BuildArgs {
        project_dir: fixture.project_dir.clone(),
        target: CanonicalTarget::parse("node").unwrap(),
        output_dir: Some(output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    })
    .unwrap();

    assert_eq!(status, ExitCode::SUCCESS);
    let manifest = ExecutionManifest::read_from_output_dir(&output_dir).unwrap().expect("execution manifest");
    assert_eq!(manifest.run_contracts.len(), 2);
    assert!(manifest.run_contracts.iter().any(|contract| contract.physical_entry.contains("alpha_entry")));
    assert!(manifest.run_contracts.iter().any(|contract| contract.physical_entry.contains("beta_entry")));
    assert!(output_dir.join("run-contract.txt").exists());
}

#[test]
fn builds_node_wasm_project() {
    let fixture = create_smoke_project_with_build(
        "legion-build-node",
        r#"{
            target: "node",
            wat: true
        }"#,
        r#"[main]
micro main() -> i64 {
    return 0
}
"#,
    );
    let output_dir = fixture.project_dir.join("dist").join("custom-node");
    let status = run(&BuildArgs {
        project_dir: fixture.project_dir.clone(),
        target: CanonicalTarget::parse("node").unwrap(),
        output_dir: Some(output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    })
    .unwrap();

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(output_dir.join("main.wasm").exists());
    assert!(output_dir.join("main.wat").exists());
    assert!(output_dir.join("main.mjs").exists());
    let manifest = ExecutionManifest::read_from_output_dir(&output_dir).unwrap().expect("execution manifest");
    assert!(
        manifest.run_contracts.iter().any(|contract| contract.physical_entry == "main.mjs" && contract.invocation == "node"),
        "expected node run contract for main.mjs, got {:?}",
        manifest.run_contracts
    );
}

#[test]
fn builds_wasi_project() {
    let fixture = create_smoke_project_with_build(
        "legion-build-wasi",
        r#"{
            target: "wasi",
            wat: true
        }"#,
        r#"[main]
micro main() -> i64 {
    return 0
}
"#,
    );
    let output_dir = fixture.project_dir.join("dist").join("custom-wasi");
    let status = run(&BuildArgs {
        project_dir: fixture.project_dir.clone(),
        target: CanonicalTarget::parse("wasi").unwrap(),
        output_dir: Some(output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    })
    .unwrap();

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(output_dir.join("main.wasi").exists());
    assert!(output_dir.join("main.wat").exists());
    let manifest = ExecutionManifest::read_from_output_dir(&output_dir).unwrap().expect("execution manifest");
    assert!(
        manifest.run_contracts.iter().any(|contract| contract.physical_entry == "main.wasi" && contract.invocation == "wasmtime"),
        "expected wasmtime run contract for main.wasi, got {:?}",
        manifest.run_contracts
    );
}

fn run_linux_elf(path: &std::path::Path) -> std::process::Output {
    #[cfg(unix)]
    {
        return std::process::Command::new(path).output().expect("run linux elf");
    }
    #[cfg(windows)]
    {
        let wsl_check = Command::new("wsl").args(["-e", "true"]).status().expect("WSL required to execute Linux ELF on Windows");
        assert!(wsl_check.success(), "WSL required to execute Linux ELF on Windows");
        let win_path = windows_path_for_wsl(path);
        let wslpath = Command::new("wsl").args(["-e", "wslpath", "-a", &win_path]).output().expect("wslpath");
        assert!(wslpath.status.success(), "wslpath failed for {}: {}", win_path, String::from_utf8_lossy(&wslpath.stderr));
        let linux_path = String::from_utf8_lossy(&wslpath.stdout).trim().to_string();
        assert!(!linux_path.is_empty(), "wslpath returned empty path for {win_path}");
        let chmod = Command::new("wsl").args(["-e", "chmod", "+x", &linux_path]).status().expect("chmod");
        assert!(chmod.success(), "chmod +x failed for {linux_path}");
        Command::new("wsl").args(["-e", &linux_path]).output().expect("wsl run elf")
    }
    #[cfg(not(any(unix, windows)))]
    {
        panic!("Linux ELF runtime verification unsupported on this host");
    }
}

#[cfg(windows)]
fn windows_path_for_wsl(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    let s = s.strip_prefix(r"\\?\").or_else(|| s.strip_prefix("//?/")).unwrap_or(&s);
    s.replace('\\', "/")
}

#[test]
fn builds_migrated_test_wasm_minimal_project() {
    let fixture = create_smoke_project_with_build(
        "legion-build-migrated-wasm-minimal",
        r#"{
            target: "wasm32-unknown-web-webassembly"
        }"#,
        r#"[main]
micro main() -> i64 {
    return 42
}
"#,
    );
    let output_dir = fixture.project_dir.join("dist").join("custom-web-wasm");
    let status = run(&BuildArgs {
        project_dir: fixture.project_dir.clone(),
        target: CanonicalTarget::parse("wasm32-unknown-web-webassembly").unwrap(),
        output_dir: Some(output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    })
    .unwrap();

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(output_dir.join("main.wasm").exists());
    assert!(output_dir.join("main.mjs").exists());
    assert!(output_dir.join("run-contracts.txt").exists());
}

#[test]
fn builds_migrated_test_wasm_hello_project() {
    let source = r#"[main]
micro main() -> i64 {
    return 42;
}
"#;
    let node_fixture = create_smoke_project_with_build(
        "legion-build-migrated-wasm-hello-node",
        r#"{
            target: "node"
        }"#,
        source,
    );
    let node_output_dir = node_fixture.project_dir.join("dist").join("custom-node");
    let node_status = run(&BuildArgs {
        project_dir: node_fixture.project_dir.clone(),
        target: CanonicalTarget::parse("node").unwrap(),
        output_dir: Some(node_output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    })
    .unwrap();

    assert_eq!(node_status, ExitCode::SUCCESS);
    assert!(node_output_dir.join("main.wasm").exists());
    assert!(node_output_dir.join("main.mjs").exists());

    let wasi_fixture = create_smoke_project_with_build(
        "legion-build-migrated-wasm-hello-wasi",
        r#"{
            target: "wasi"
        }"#,
        source,
    );
    let wasi_output_dir = wasi_fixture.project_dir.join("dist").join("custom-wasi");
    let wasi_status = run(&BuildArgs {
        project_dir: wasi_fixture.project_dir.clone(),
        target: CanonicalTarget::parse("wasi").unwrap(),
        output_dir: Some(wasi_output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    })
    .unwrap();

    assert_eq!(wasi_status, ExitCode::SUCCESS);
    assert!(wasi_output_dir.join("main.wasi").exists());
    assert!(wasi_output_dir.join("run-contracts.txt").exists());
}

const WITNESS_SMOKE_SOURCE: &str = r#"trait Animal {
    micro make_sound(): utf8
}

class Dog {
}

imply Dog: Animal {
    micro make_sound() -> utf8 {
        return "woof"
    }
}

[main]
micro main() -> i64 {
    return 0
}
"#;

#[test]
fn builds_wasm_wasi_witness_dispatch() {
    let fixture = create_smoke_project_with_build(
        "legion-build-wasi-witness",
        r#"{
            target: "wasi"
        }"#,
        WITNESS_SMOKE_SOURCE,
    );
    let output_dir = fixture.project_dir.join("dist").join("wasi-witness");
    let status = run(&BuildArgs {
        project_dir: fixture.project_dir.clone(),
        target: CanonicalTarget::parse("wasi").unwrap(),
        output_dir: Some(output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    })
    .unwrap();

    assert_eq!(status, ExitCode::SUCCESS);
    let wasm_path = output_dir.join("main.wasi");
    assert!(wasm_path.exists());
    let output = run_wasmtime(&wasm_path);
    assert!(output.status.success(), "exit={:?} stderr={}", output.status, String::from_utf8_lossy(&output.stderr));
}

fn run_java_witness_jar(jar_path: &std::path::Path) -> Option<std::process::Output> {
    use std::{io::Read, process::Command};
    let file = std::fs::File::open(jar_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut main_class = None::<String>;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).ok()?;
        let entry_name = match entry.enclosed_name() {
            Some(path) => path.to_string_lossy().into_owned(),
            None => continue,
        };
        if entry_name.eq_ignore_ascii_case("META-INF/MANIFEST.MF") {
            let mut manifest = String::new();
            entry.read_to_string(&mut manifest).ok()?;
            main_class = manifest.lines().find_map(|line| line.strip_prefix("Main-Class:").map(|value| value.trim().to_string()));
            break;
        }
    }
    let main_class = main_class?;
    Command::new("java").arg("-cp").arg(windows_friendly_path(jar_path)).arg(main_class).output().ok()
}

fn windows_friendly_path(path: &std::path::Path) -> std::path::PathBuf {
    let rendered = path.to_string_lossy();
    if let Some(stripped) = rendered.strip_prefix(r"\\?\") { std::path::PathBuf::from(stripped) } else { path.to_path_buf() }
}

fn run_java_from_run_contracts(output_dir: &std::path::Path) -> Option<std::process::Output> {
    run_java_from_run_contracts_filtered(output_dir, |_| true)
}

fn run_java_suspend_from_run_contracts(output_dir: &std::path::Path) -> Option<std::process::Output> {
    run_java_from_run_contracts_filtered(output_dir, |physical| physical.contains("suspend"))
        .or_else(|| run_java_from_run_contracts(output_dir))
}

fn run_java_from_run_contracts_filtered(output_dir: &std::path::Path, filter: impl Fn(&str) -> bool) -> Option<std::process::Output> {
    use std::process::Command;
    let contracts = RunContract::read_all_from_output_dir(output_dir).ok()?;
    for contract in &contracts {
        if contract.physical_entry.is_empty() || !filter(&contract.physical_entry) {
            continue;
        }
        let jar_path = windows_friendly_path(&output_dir.join(&contract.physical_entry));
        if !jar_path.is_file() || !jar_path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("jar"))
        {
            continue;
        }
        let main_class = if contract.logical_entry.is_empty() {
            jar_path.file_stem().and_then(|value| value.to_str()).map(str::to_string)?
        }
        else {
            contract.logical_entry.clone()
        };
        if let Some(output) = Command::new("java").arg("-jar").arg(&jar_path).output().ok() {
            if output.status.success() {
                return Some(output);
            }
        }
        return Command::new("java").arg("-cp").arg(&jar_path).arg(main_class).output().ok();
    }
    None
}

fn jar_path_from_run_contracts(output_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let contracts = RunContract::read_all_from_output_dir(output_dir).ok()?;
    for contract in &contracts {
        if contract.physical_entry.is_empty() {
            continue;
        }
        let path = output_dir.join(&contract.physical_entry);
        if path.is_file() && path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("jar")) {
            return Some(path);
        }
    }
    None
}

fn run_wasmtime(path: &std::path::Path) -> std::process::Output {
    Command::new("wasmtime")
        .args(["run", "-W", "gc", "-W", "max-memory-size=16777216"])
        .arg(path)
        .output()
        .unwrap_or_else(|error| panic!("wasmtime required to execute WASI witness sample: {error}"))
}

const AWAIT_FUTURE_SUSPEND_SOURCE: &str = include_str!("fixtures/runtime_smoke/await_future.valkyrie");
const SUSPEND_TRAIT_COMBO_SOURCE: &str = include_str!("fixtures/runtime_smoke/suspend_trait_combo.valkyrie");

fn multi_lane_suspend_manifest() -> String {
    r#"{
    name: "test_suspend",
    version: "0.1.0",
    dependencies: {
        "std": false,
        "core": false
    },
    build: [
        { target: "clr", msil: true },
        { target: "jvm-openjdk-unknown-managed" },
        { target: "node" },
        { target: "wasi" },
        { target: "x86_64-unknown-linux-gnu" },
        { target: "x86_64-pc-windows-msvc" }
    ]
}
"#
    .to_string()
}

fn command_exists(command: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|dir| {
            let base = dir.join(command);
            base.is_file()
                || [".exe", ".cmd", ".bat", ".com"].iter().map(|ext| dir.join(format!("{command}{ext}"))).any(|candidate| candidate.is_file())
        })
    })
}
