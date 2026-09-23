mod support;

use nyar::{
    BackendInputKind, BinaryArch, BinaryFlavor, BinaryTarget, HostProjectionBoundary, PartitionBackendRequirement, ReferenceManagement,
    TargetFamily, TargetLane,
};
use nyar_emitter::{
    BackendBoundaryError, BackendDispatchKind, BackendInputShape, BackendRoute, testing::compile_lowered_backend_input, validate_backend_input,
    validate_dispatch_for_route,
};
use std::process::Command;
use tempfile::tempdir;

use crate::support::{
    compilation_options, demo_jvm_input, demo_wasm_input, demo_wasm_input_with_imports, demo_wasm_input_with_preview, jvm_requirement,
    wasm_requirement,
};

#[test]
fn creates_jvm_run_contract_via_bundled_compiler() {
    let output_dir = tempdir().expect("temp dir");
    let options = compilation_options(BinaryTarget::new(TargetFamily::Jvm, BinaryArch::Any, BinaryFlavor::ManagedClr), "demo");
    let input = demo_jvm_input(output_dir.path());
    let report = compile_lowered_backend_input("demo", jvm_requirement(options.target.clone()), input, false, &options).expect("compile ok");

    let contract = report.run_contracts.into_iter().next().expect("missing run contract");
    assert_eq!(contract.physical_entry, "demo.jar");
    assert_eq!(contract.invocation, "java");
    assert_eq!(contract.validate, "java -jar demo.jar");
    assert!(contract.logical_entry.ends_with(".Main") || contract.logical_entry == "Main");
}

#[test]
fn creates_node_and_wasi_run_contracts_via_bundled_compiler() {
    let output_dir = tempdir().expect("temp dir");
    let node_options = compilation_options(BinaryTarget::new(TargetFamily::Wasm, BinaryArch::Any, BinaryFlavor::Native), "demo_node");
    let node_input = demo_wasm_input(output_dir.path(), HostProjectionBoundary::WasmJsGlue);
    let node_report = compile_lowered_backend_input(
        "demo_node",
        wasm_requirement(node_options.target.clone(), HostProjectionBoundary::WasmJsGlue),
        node_input,
        false,
        &node_options,
    )
    .expect("node compile ok");
    let node_contract = node_report.run_contracts.into_iter().next().expect("missing node contract");
    assert_eq!(node_contract.logical_entry, "main");
    assert_eq!(node_contract.physical_entry, "demo_node.mjs");
    assert_eq!(node_contract.invocation, "node");

    let wasi_options = compilation_options(BinaryTarget::new(TargetFamily::Wasm, BinaryArch::Any, BinaryFlavor::Native), "demo_wasi");
    let wasi_input = demo_wasm_input(output_dir.path(), HostProjectionBoundary::WasiComponent);
    let wasi_report = compile_lowered_backend_input(
        "demo_wasi",
        wasm_requirement(wasi_options.target.clone(), HostProjectionBoundary::WasiComponent),
        wasi_input,
        false,
        &wasi_options,
    )
    .expect("wasi compile ok");
    let wasi_contract = wasi_report.run_contracts.into_iter().next().expect("missing wasi contract");
    assert_eq!(wasi_contract.logical_entry, "_start");
    assert_eq!(wasi_contract.physical_entry, "demo_wasi.wasi");
    assert_eq!(wasi_contract.validate, "wasmtime run -W gc -W wmemcheck -W max-memory-size=16777216 demo_wasi.wasi");

    let component_path = output_dir.path().join("demo_wasi.wasi");
    let wit_dump = Command::new("wasm-tools")
        .args(["component", "wit", component_path.to_string_lossy().as_ref()])
        .output()
        .expect("run wasm-tools component wit");
    assert!(wit_dump.status.success(), "stderr={}", String::from_utf8_lossy(&wit_dump.stderr));
    let wit_text = String::from_utf8(wit_dump.stdout).expect("utf8 component wit");
    assert!(wit_text.contains("world "), "wit={wit_text}");
    assert!(wit_text.contains("export wasi:cli/run@0.2.12;") || wit_text.contains("wasi:cli/run@0.2.12"), "wit={wit_text}");
}

#[test]
fn creates_wasip3_run_contract_and_packages_component() {
    let output_dir = tempdir().expect("temp dir");
    let options = compilation_options(BinaryTarget::new(TargetFamily::Wasm, BinaryArch::Any, BinaryFlavor::Native), "demo_wasip3");
    let input = demo_wasm_input_with_preview(
        output_dir.path(),
        HostProjectionBoundary::WasiComponent,
        nyar_emitter::nyar_backend_wasi::WasiPreview::Preview3,
    );
    let report = compile_lowered_backend_input(
        "demo_wasip3",
        wasm_requirement(options.target.clone(), HostProjectionBoundary::WasiComponent),
        input,
        false,
        &options,
    )
    .expect("wasip3 compile ok");
    let contract = report.run_contracts.into_iter().next().expect("missing wasip3 run contract");
    assert_eq!(contract.physical_entry, "demo_wasip3.wasi");
    assert!(contract.validate.contains("-S p3"), "validate={}", contract.validate);

    let component_path = output_dir.path().join("demo_wasip3.wasi");
    let wit_dump = Command::new("wasm-tools")
        .args(["component", "wit", component_path.to_string_lossy().as_ref()])
        .output()
        .expect("run wasm-tools component wit");
    assert!(wit_dump.status.success(), "stderr={}", String::from_utf8_lossy(&wit_dump.stderr));
    let wit_text = String::from_utf8(wit_dump.stdout).expect("utf8 component wit");
    assert!(wit_text.contains("wasi:cli/run@0.3.0"), "wit={wit_text}");

    let run = Command::new("wasmtime")
        .args(["run", "-W", "gc", "-S", "p3", component_path.to_string_lossy().as_ref()])
        .output()
        .expect("run wasmtime wasip3");
    assert!(run.status.success(), "stderr={}", String::from_utf8_lossy(&run.stderr));
}

#[test]
fn packages_wasi_component_with_mirrored_imports() {
    let output_dir = tempdir().expect("temp dir");
    let options = compilation_options(BinaryTarget::new(TargetFamily::Wasm, BinaryArch::Any, BinaryFlavor::Native), "demo_wasi_imports");
    let imports = vec![
        ("wasi:clocks/monotonic-clock".to_string(), "now".to_string()),
        ("wasi:io/streams".to_string(), "blocking-write-and-flush".to_string()),
    ];
    let input = demo_wasm_input_with_imports(output_dir.path(), HostProjectionBoundary::WasiComponent, imports);
    let _report = compile_lowered_backend_input(
        "demo_wasi_imports",
        wasm_requirement(options.target.clone(), HostProjectionBoundary::WasiComponent),
        input,
        false,
        &options,
    )
    .expect("wasi compile ok");

    let component_path = output_dir.path().join("demo_wasi_imports.wasi");
    let wit_dump = Command::new("wasm-tools")
        .args(["component", "wit", component_path.to_string_lossy().as_ref()])
        .output()
        .expect("run wasm-tools component wit");
    assert!(wit_dump.status.success(), "stderr={}", String::from_utf8_lossy(&wit_dump.stderr));
    let wit_text = String::from_utf8(wit_dump.stdout).expect("utf8 component wit");
    assert!(wit_text.contains("import"), "wit={wit_text}");
    assert!(wit_text.contains("now: func() -> u64;") || wit_text.contains("now: func() ->"), "wit={wit_text}");
    assert!(wit_text.contains("blocking-write-and-flush") && wit_text.contains("func"), "wit={wit_text}");
    assert!(wit_text.contains("export wasi:cli/run@0.2.12;") || wit_text.contains("wasi:cli/run@0.2.12"), "wit={wit_text}");
}

#[test]
fn rejects_unsupported_backend_requirement() {
    let output_dir = tempdir().expect("temp dir");
    let target = BinaryTarget::new(TargetFamily::Gpu, BinaryArch::X64, BinaryFlavor::Native);
    let options = compilation_options(target.clone(), "demo");
    let input = demo_jvm_input(output_dir.path());
    let requirement = PartitionBackendRequirement {
        backend_name: "missing-backend".to_string(),
        interpreter: nyar::Identifier::new("missing"),
        fragment: nyar::Identifier::new("functions"),
        lane: TargetLane::Clr,
        input_kind: BackendInputKind::WasmModule,
        target: BinaryTarget::new(TargetFamily::Wasm, BinaryArch::Any, BinaryFlavor::Native),
        host_boundary: HostProjectionBoundary::WasmJsGlue,
        reference_management: ReferenceManagement::HostGc,
    };
    let error =
        compile_lowered_backend_input("demo", requirement, input, false, &options).expect_err("should reject unsupported backend requirement");
    let msg = error.to_string();
    assert!(
        msg.contains("尚未接入") || msg.contains("emitter") || msg.contains("driver compiler") || msg.contains("missing-backend"),
        "unexpected error: {msg}"
    );
}

#[test]
fn backend_rejects_open_row_evidence() {
    let error =
        validate_backend_input(BackendInputShape { contains_open_row_evidence: true, contains_unresolved_nominal_checks: false }).unwrap_err();
    assert_eq!(error, BackendBoundaryError::OpenRowEvidence);
}

#[test]
fn backend_rejects_unresolved_nominal_checks() {
    let error =
        validate_backend_input(BackendInputShape { contains_open_row_evidence: false, contains_unresolved_nominal_checks: true }).unwrap_err();
    assert_eq!(error, BackendBoundaryError::UnresolvedNominalCheck);
}

#[test]
fn backend_rejects_open_trait_dispatch_when_route_cannot_lower_it() {
    let error = validate_dispatch_for_route(BackendRoute::StaticOnly, BackendDispatchKind::Witness).unwrap_err();
    assert_eq!(error, BackendBoundaryError::UnsupportedTraitDispatch { route: BackendRoute::StaticOnly });
}

#[test]
fn backend_rejects_open_effect_dispatch_when_route_cannot_lower_it() {
    let error = validate_dispatch_for_route(BackendRoute::WitnessCapable, BackendDispatchKind::EffectHandler).unwrap_err();
    assert_eq!(error, BackendBoundaryError::UnsupportedEffectDispatch { route: BackendRoute::WitnessCapable });
}

#[test]
fn node_launcher_is_free_of_banner_and_cli_pollution() {
    let output_dir = tempdir().expect("temp dir");
    let node_options = compilation_options(BinaryTarget::new(TargetFamily::Wasm, BinaryArch::Any, BinaryFlavor::Native), "demo_node");
    let node_input = demo_wasm_input(output_dir.path(), HostProjectionBoundary::WasmJsGlue);
    let _report = compile_lowered_backend_input(
        "demo_node",
        wasm_requirement(node_options.target.clone(), HostProjectionBoundary::WasmJsGlue),
        node_input,
        false,
        &node_options,
    )
    .expect("node compile ok");

    let mjs_path = output_dir.path().join("demo_node.mjs");
    let mjs_content = std::fs::read_to_string(&mjs_path).expect("read .mjs file");

    assert!(!mjs_content.contains("LEGION_BOOTSTRAP_HOST"), "LEGION_BOOTSTRAP_HOST 必须不存在");
    assert!(!mjs_content.contains("LEGION_HOST"), "LEGION_HOST 必须不存在");
    assert!(!mjs_content.contains("host_legion_compile_from_plan"), "host_legion_compile_from_plan 必须不存在");
    assert!(!mjs_content.contains("legion.von"), "legion.von 必须不存在");
    assert!(!mjs_content.contains("Valkyrie legion CLI"), "Valkyrie legion CLI banner 必须不存在");
    assert!(!mjs_content.contains("legion.tools"), "legion.tools project_name 必须不存在");
    assert!(!mjs_content.contains("cliState"), "cliState 全局状态必须不存在");
    assert!(!mjs_content.contains("spawnSync"), "spawnSync 导入必须不存在");
    assert!(!mjs_content.contains("\"--version\""), "--version 子命令必须不存在");
    assert!(!mjs_content.contains("\"--help\""), "--help 子命令必须不存在");
    assert!(mjs_content.contains("exports.main ?? exports._start"), "入口分派必须存在");
}

#[test]
fn node_launcher_renders_only_declared_imports() {
    let output_dir = tempdir().expect("temp dir");
    let node_options = compilation_options(BinaryTarget::new(TargetFamily::Wasm, BinaryArch::Any, BinaryFlavor::Native), "demo_imports");
    let imports = vec![("env".to_string(), "emit_byte".to_string()), ("env".to_string(), "emit_i32".to_string())];
    let node_input = demo_wasm_input_with_imports(output_dir.path(), HostProjectionBoundary::WasmJsGlue, imports);
    let _report = compile_lowered_backend_input(
        "demo_imports",
        wasm_requirement(node_options.target.clone(), HostProjectionBoundary::WasmJsGlue),
        node_input,
        false,
        &node_options,
    )
    .expect("node compile ok");

    let mjs_path = output_dir.path().join("demo_imports.mjs");
    let mjs_content = std::fs::read_to_string(&mjs_path).expect("read .mjs file");

    assert!(mjs_content.contains("emit_byte"), "声明的 import emit_byte 必须出现在 importObject 中");
    assert!(mjs_content.contains("emit_i32"), "声明的 import emit_i32 必须出现在 importObject 中");
    assert!(mjs_content.contains("output_bytes"), "emit_byte 触发的 output_bytes 逻辑必须存在");
    assert!(mjs_content.contains("process.stdout.write"), "stdout 写入路径必须存在");
    assert!(!mjs_content.contains("host_legion_compile_from_plan"), "伪导入 host_legion_compile_from_plan 不应被注入");
    assert!(mjs_content.contains("exports.main ?? exports._start"), "入口分派必须存在");
}

#[test]
fn node_launcher_without_imports_has_clean_stdout() {
    let output_dir = tempdir().expect("temp dir");
    let node_options = compilation_options(BinaryTarget::new(TargetFamily::Wasm, BinaryArch::Any, BinaryFlavor::Native), "demo_clean");
    let node_input = demo_wasm_input(output_dir.path(), HostProjectionBoundary::WasmJsGlue);
    let _report = compile_lowered_backend_input(
        "demo_clean",
        wasm_requirement(node_options.target.clone(), HostProjectionBoundary::WasmJsGlue),
        node_input,
        false,
        &node_options,
    )
    .expect("node compile ok");

    let mjs_path = output_dir.path().join("demo_clean.mjs");
    let mjs_content = std::fs::read_to_string(&mjs_path).expect("read .mjs file");

    assert!(!mjs_content.contains("process.argv[2]"), "无 imports 时不应有 CLI 参数解析");
    assert!(!mjs_content.contains("parseInt"), "无 imports 时不应有 parseInt 参数解析");
    assert!(!mjs_content.contains("Valkyrie"), "Valkyrie banner 不应污染 stdout");
    assert!(!mjs_content.contains("legion CLI"), "legion CLI banner 不应污染 stdout");
    assert!(mjs_content.contains("wasmResolveInstance(wasmBytes, {})"), "无 imports 时应使用空 importObject");
    assert!(mjs_content.contains("exports.main ?? exports._start"), "入口分派必须存在");
}
