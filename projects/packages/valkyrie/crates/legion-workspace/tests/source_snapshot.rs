//! Migrated from `src/source_snapshot.rs` (`tests`).

use legion_workspace::{planner::PlannedSemanticSourceGroup, source_snapshot::*};
use miette::Result;
use nyar_types::compiler_input::CompilerSourceGroup;
use std::{fs, path::Path};
use tempfile::tempdir;

#[test]
fn unindexable_entry_source_does_not_become_a_silent_empty_program() {
    let directory = tempdir().expect("创建源码目录");
    // 只有维护性文本、没有可索引入口声明的入口组必须结构化失败。
    // 旧行为把入口组整文件下沉，会静默编译出一个没有程序入口的空模块。
    let source = "// 锟斤拷 Ã© â€™ `r`n \u{fffd}\n".repeat(5000);
    let group = source_group(directory.path(), "application", &source, &[], true);
    let groups = compile_source_snapshot(&[group]).expect("Resolver 保留输入原文");
    let output = directory.path().join("output");
    fs::create_dir_all(&output).expect("创建产物目录");
    let error = compile_snapshot(&groups, &output).expect_err("入口组缺少显式入口必须结构化失败");
    assert!(error.to_string().contains("declares no `[export]` or `[main]` entry"), "{error}");
}

#[test]
fn snapshot_paths_are_manifest_relative_and_reject_outside_sources() {
    let directory = tempdir().expect("创建源码目录");
    let group = source_group(directory.path(), "application", "micro answer() { return 1 }", &[], true);
    let snapshot = compile_source_snapshot(std::slice::from_ref(&group)).expect("合法闭包");
    assert_eq!(snapshot[0].sources[0].path, "application.v");
    assert!(!Path::new(&snapshot[0].sources[0].path).is_absolute());
    let outside = tempdir().expect("创建独立源码目录");
    let mut invalid = group;
    invalid.manifest_dir = outside.path().to_path_buf();
    let error = compile_source_snapshot(&[invalid]).expect_err("不能泄漏仓外路径作为逻辑身份");
    assert!(error.to_string().contains("manifest"));
}

fn source_group(root: &Path, name: &str, source: &str, dependencies: &[&str], is_entry: bool) -> PlannedSemanticSourceGroup {
    let source_file = root.join(format!("{name}.v"));
    fs::write(&source_file, source).expect("写入当前源码");
    PlannedSemanticSourceGroup {
        dependency_key: name.to_owned(),
        name: name.to_owned(),
        manifest_dir: root.to_path_buf(),
        source_files: vec![source_file],
        direct_dependencies: dependencies.iter().map(|dependency| (*dependency).to_owned()).collect(),
        is_entry,
    }
}

fn micro_declaration(dependency_key: &str, source_path: &str, name: &str) -> rusty_valkyrie::DeclarationKey {
    rusty_valkyrie::DeclarationKey {
        dependency_key: dependency_key.to_owned(),
        source_path: source_path.to_owned(),
        namespace: Vec::new(),
        kind: rusty_valkyrie::DeclarationKind::Micro,
        owner: None,
        name: name.to_owned(),
    }
}

fn compile_snapshot(groups: &[CompilerSourceGroup], output_dir: &Path) -> Result<rusty_valkyrie::CompilerArtifactReport> {
    compile_snapshot_with_bindings(groups, output_dir, &[])
}

fn compile_snapshot_with_bindings(
    groups: &[CompilerSourceGroup],
    output_dir: &Path,
    host_bindings: &[rusty_valkyrie::CompilerHostProviderBinding],
) -> Result<rusty_valkyrie::CompilerArtifactReport> {
    let build_context = rusty_valkyrie::CompilerBuildContext::new(
        "wasm32",
        rusty_valkyrie::CanonicalTarget::parse("node").expect("正式 Node 目标"),
        nyar::ClrSuspendStrategy::default(),
        nyar_emitter::nyar_backend_wasi::WasmPackageKind::Binary,
    )
    .with_selected_host_providers(host_bindings.to_vec());
    rusty_valkyrie::compile_source_groups_to_artifacts(
        &rusty_valkyrie::ValkyrieCompiler::default(),
        &nyar_types::compiler_input::CompilerSourceInput::application(groups.to_vec()),
        &build_context,
        output_dir,
        "application",
        false,
        false,
    )
}

#[test]
fn single_group_keeps_resolver_identity() {
    let directory = tempdir().expect("创建源码目录");
    let group = source_group(directory.path(), "application", "[main] micro main() -> bool { return true }", &[], true);
    let groups = compile_source_snapshot(&[group]).expect("Resolver 形成完整源码组");
    assert_eq!(groups[0].dependency_key, "application");
    assert_eq!(groups[0].name, "application");
    let output = directory.path().join("output");
    fs::create_dir_all(&output).expect("创建产物目录");
    let report = compile_snapshot(&groups, &output).expect("当前源码进入正式产物入口");
    assert!(!report.driver.artifacts.artifacts.is_empty(), "编译成功必须具有产物");
    assert!(!report.driver.run_contracts.is_empty(), "二进制产物必须具有执行合同");
    assert!(!report.evidence.semantic_closure_hash.is_empty(), "编译证据必须包含源码闭包摘要");
    assert!(!report.evidence.canonical_surface_hash.is_empty(), "编译证据必须包含 canonical 表面摘要");
}

#[test]
fn dependency_groups_link_from_current_source() {
    let directory = tempdir().expect("创建源码目录");
    let dependency = source_group(directory.path(), "library", "micro answer() -> bool { return true }", &[], false);
    let application = source_group(directory.path(), "application", "[main] micro main() -> bool { return answer() }", &["library"], true);
    let groups = compile_source_snapshot(&[dependency, application]).expect("Resolver 形成完整源码组");
    assert_eq!(groups[1].direct_dependencies, ["library"]);
    assert_eq!(groups[0].dependency_key, "library");
    let output = directory.path().join("output");
    fs::create_dir_all(&output).expect("创建产物目录");
    let report = compile_snapshot(&groups, &output).expect("当前依赖源码必须完成正式编译");
    assert!(!report.driver.artifacts.artifacts.is_empty());
}

#[test]
fn changed_source_is_not_replaced_by_a_previous_success() {
    let directory = tempdir().expect("创建源码目录");
    let group = source_group(directory.path(), "application", "[main] micro main() -> bool { return true }", &[], true);
    let output = directory.path().join("output");
    fs::create_dir_all(&output).expect("创建产物目录");
    let groups = compile_source_snapshot(std::slice::from_ref(&group)).expect("Resolver 形成完整源码组");
    compile_snapshot(&groups, &output).expect("首次源码编译成功");
    fs::write(&group.source_files[0], "micro main(").expect("替换为无效源码");
    let groups = compile_source_snapshot(&[group]).expect("Resolver 读取当前源码");
    compile_snapshot(&groups, &output).expect_err("不得复用已存在的旧产物掩盖当前源码失败");
}

#[test]
fn unresolved_dependency_does_not_flatten_the_source_closure() {
    let directory = tempdir().expect("创建源码目录");
    let group = source_group(directory.path(), "application", "[main] micro main() -> bool { return true }", &["missing"], true);
    let groups = compile_source_snapshot(&[group]).expect("Resolver 形成完整源码组");
    let output = directory.path().join("output");
    fs::create_dir_all(&output).expect("创建产物目录");
    let error = compile_snapshot(&groups, &output).expect_err("未解析依赖必须在 Compiler 边界失败");
    assert!(error.to_string().contains("missing"), "{error}");
}

#[test]
fn missing_source_file_fails_instead_of_compiling_a_partial_group() {
    let directory = tempdir().expect("创建源码目录");
    let mut group = source_group(directory.path(), "application", "micro main() -> bool { return true }", &[], true);
    group.source_files.push(directory.path().join("missing.v"));
    let error = compile_source_snapshot(&[group]).expect_err("必须拒绝不完整源码组");
    assert!(error.to_string().contains("读取源码失败"), "{error}");
}

#[test]
fn empty_snapshot_is_not_a_successful_program() {
    let error = compile_source_snapshot(&[]).expect_err("必须拒绝空快照");
    assert!(error.to_string().contains("semantic source group plan is empty"), "{error}");
}

#[test]
fn selected_host_provider_rewrites_contract_calls_in_snapshot_closure() {
    let directory = tempdir().expect("创建源码目录");
    let adaptor =
        source_group(directory.path(), "adaptor", "[host_provider(demo::write)] micro write(value: bool) -> bool { return value }", &[], false);
    let application = source_group(
        directory.path(),
        "demo",
        "[host_contract] micro write(value: bool) -> bool { } \
 [main] micro main() -> bool { return write(true) }",
        &["adaptor"],
        true,
    );
    let groups = compile_source_snapshot(&[adaptor, application]).expect("Resolver 形成完整源码组");
    let output = directory.path().join("output");
    fs::create_dir_all(&output).expect("创建产物目录");
    let report = compile_snapshot_with_bindings(
        &groups,
        &output,
        &[rusty_valkyrie::CompilerHostProviderBinding {
            contract: micro_declaration("demo", "demo.v", "write"),
            provider: micro_declaration("adaptor", "adaptor.v", "write"),
        }],
    )
    .expect("Resolver 选定的 host provider 必须进入 Compiler 合同");
    assert!(!report.driver.artifacts.artifacts.is_empty(), "host contract 闭合后必须生成产物");
    assert!(!report.evidence.host_provider_binding_hash.is_empty(), "host provider 绑定必须进入编译证据");
}

#[test]
fn resolver_keeps_templates_and_compiler_expands_by_arch() {
    let directory = tempdir().expect("创建源码目录");
    let template = r#"<% match arch %>
<% case "wasm32" %>
[main] micro main() -> bool { return true }
<% else %>
[main] micro main() -> bool { return false }
<% end %>"#;
    let group = source_group(directory.path(), "application", template, &[], true);
    let groups = compile_source_snapshot(&[group]).expect("Resolver 只拼接源码，不展开模板");
    assert!(groups[0].sources.iter().any(|file| file.source.contains("<% match ")));
    let output = directory.path().join("output");
    fs::create_dir_all(&output).expect("创建产物目录");
    let report = compile_snapshot(&groups, &output).expect("结构化 TGrammar 应由 Compiler 按 arch 展开");
    assert!(!report.driver.artifacts.artifacts.is_empty(), "编译成功必须具有产物");
}

#[test]
fn unreachable_dependency_inventory_syntax_error_does_not_block_entry_snapshot() {
    let directory = tempdir().expect("创建源码目录");
    let good = directory.path().join("library/good.v");
    let broken = directory.path().join("library/broken.v");
    fs::create_dir_all(good.parent().expect("library dir")).expect("创建依赖目录");
    fs::write(&good, "micro answer() -> bool { return true }").expect("写入可达依赖源码");
    fs::write(&broken, "micro broken() { value = }").expect("写入不可达坏依赖源码");
    let dependency = PlannedSemanticSourceGroup {
        dependency_key: "library".into(),
        name: "library".into(),
        manifest_dir: directory.path().join("library"),
        source_files: vec![good, broken],
        direct_dependencies: vec![],
        is_entry: false,
    };
    let application_file = directory.path().join("application.v");
    fs::write(&application_file, "[main] micro main() -> bool { return answer() }").expect("写入入口源码");
    let application = PlannedSemanticSourceGroup {
        dependency_key: "application".into(),
        name: "application".into(),
        manifest_dir: directory.path().to_path_buf(),
        source_files: vec![application_file],
        direct_dependencies: vec!["library".into()],
        is_entry: true,
    };
    let groups = compile_source_snapshot(&[dependency, application]).expect("Resolver 形成语义闭包快照");
    // Resolver 必须保留完整文件输入；只有 Compiler 决定语义选择。
    assert_eq!(groups[0].sources.len(), 2);
    let output = directory.path().join("output");
    fs::create_dir_all(&output).expect("创建产物目录");
    compile_snapshot(&groups, &output).expect("不可达库存语法错误不得阻塞入口");
}
