//! 真实 JVM 宿主冒烟：emitter → `.class`/`.jar` → `java -jar`。
//!
//! 不经临时 mjs / `javap`；二进制检查走 `std-data`（与 `legion spy jvm` 同源）。

mod support;

use std::process::Command;

use nyar::{
    BinaryArch, BinaryFlavor, BinaryTarget, CapabilityTag, ExternalCallArgument, ExternalCallEdge, ExternalImportLink, Identifier,
    QualifiedName, RewriteTheory, TargetFamily, TheoryBundle,
};
use nyar_emitter::{
    FragmentSubmission, LoweredBackendInput,
    nyar_backend_jvm::{JvmBinaryBackendInput, JvmInstruction, JvmJarPackage, decode_instructions},
    testing::{compile_lowered_backend_input, lower_fragment_to_jvm_class},
};
use tempfile::tempdir;

use crate::support::{compilation_options, demo_jvm_input, jvm_requirement};

fn java_available() -> bool {
    Command::new("java").arg("-version").output().map(|output| output.status.success()).unwrap_or(false)
}

#[test]
fn emits_runnable_jar_verified_by_host_java() {
    if !java_available() {
        eprintln!("skip jvm host runtime: java not on PATH");
        return;
    }

    let output_dir = tempdir().expect("temp dir");
    let options = compilation_options(BinaryTarget::new(TargetFamily::Jvm, BinaryArch::Any, BinaryFlavor::ManagedClr), "demo");
    let input = demo_jvm_input(output_dir.path());
    let report = compile_lowered_backend_input("demo", jvm_requirement(options.target.clone()), input, false, &options).expect("compile ok");
    let contract = report.run_contracts.into_iter().next().expect("run contract");
    assert_eq!(contract.validate, "java -jar demo.jar");

    let jar_path = output_dir.path().join("demo.jar");
    assert!(jar_path.is_file(), "missing {}", jar_path.display());

    let class_path = output_dir.path().join("demo").join("Main.class");
    assert!(class_path.is_file(), "class should land under package path {}", class_path.display());

    let jar_bytes = std::fs::read(&jar_path).expect("read jar");
    let package = JvmJarPackage::from_bytes("demo.jar", &jar_bytes).expect("decode jar");
    assert_eq!(package.main_class.as_deref(), Some("demo.Main"));
    let main_class = package.read_class("demo/Main").expect("read class").expect("demo/Main present");
    let main_index = main_class.methods.iter().position(|method| method.name == "main").expect("main method");
    let code = main_class.raw_method_code.get(main_index).and_then(|code| code.as_ref()).expect("main code");
    let decoded = decode_instructions(code, &main_class.constant_pool);
    assert!(!decoded.is_empty(), "spy-equivalent decode should see instructions");

    let run = Command::new("java").arg("-jar").arg(&jar_path).output().expect("java -jar");
    assert!(
        run.status.success(),
        "java -jar failed: status={:?}\nstdout={}\nstderr={}",
        run.status,
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn lowers_external_println_fragment_to_host_runnable_jar() {
    if !java_available() {
        eprintln!("skip jvm println host: java not on PATH");
        return;
    }

    let main = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("main")]);
    let console_write_line = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("console_write_line")]);
    let class_file = lower_fragment_to_jvm_class(&FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("main"),
        exported_operations: vec![main.clone()],
        required_capabilities: vec![CapabilityTag::new("host-interop")],
        theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
        entry_operation: Some(main.clone()),
        external_import_links: std::collections::BTreeMap::from([(
            console_write_line.clone(),
            ExternalImportLink::host(
                Some(Identifier::new("jvm")),
                vec!["java/lang/System".to_string(), "out".to_string(), "java/io/PrintStream".to_string(), "println".to_string()],
            ),
        )]),
        external_call_edges: vec![ExternalCallEdge::new(
            main,
            console_write_line,
            vec![ExternalCallArgument::StringLiteral("hello from jvm host".to_string())],
        )],
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        control_flow: None,
        suspend_runtime: None,
        witness_tables: Vec::new(),
        witness_calls: Vec::new(),
        ..Default::default()
    })
    .expect("lower jvm class");

    assert!(class_file.methods.iter().any(|method| {
        method.name == "main"
            && method.code.as_ref().is_some_and(|code| {
                code.instructions
                    .iter()
                    .any(|instruction| matches!(instruction, JvmInstruction::InvokeStatic(reference) if reference.name.starts_with("entry_")))
            })
    }));

    let output_dir = tempdir().expect("temp dir");
    let options = compilation_options(BinaryTarget::new(TargetFamily::Jvm, BinaryArch::Any, BinaryFlavor::ManagedClr), "demo_print");
    let input = LoweredBackendInput::jvm(JvmBinaryBackendInput {
        class_file,
        output_dir: output_dir.path().to_path_buf(),
        emit_class_file: true,
        control_flow: None,
        companion_classes: Vec::new(),
    });
    compile_lowered_backend_input("demo_print", jvm_requirement(options.target.clone()), input, false, &options).expect("compile ok");

    let jar_path = output_dir.path().join("demo_print.jar");
    let run = Command::new("java").arg("-jar").arg(&jar_path).output().expect("java -jar");
    assert!(
        run.status.success(),
        "java -jar failed: status={:?}\nstdout={}\nstderr={}",
        run.status,
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "hello from jvm host");
}
