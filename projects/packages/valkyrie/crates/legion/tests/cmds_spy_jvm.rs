//! `legion spy jvm` 子命令的集成测试。
//!
//! 在内存中构造最小 `.class` 与 `.jar` 二进制（经 `acorn-jvm` 编码），
//! 验证 `spy jvm` 的列表、反汇编与 JSON 输出能力，不依赖外部文件。
#![cfg(feature = "format-spy")]

use acorn_jvm::{
    class::{JvmClassFile, JvmMethodDescriptor, JvmTypeDescriptor},
    jar::JvmJarPackage,
};
use legion::{SpyMode, SpyOptions, SpyTargetOptions, run_spy};
use tempfile::TempDir;

/// 构造一个含两个方法的 `class` 模型并编码为二进制。
///
/// - `compute`() -> int：由编码器生成默认方法体（`iconst_0` + `ireturn`）。
/// - `greet`() -> void：由编码器生成默认方法体（`return`）。
fn build_sample_class_bytes() -> Vec<u8> {
    let mut class = JvmClassFile::new("demo/Sample");
    class.push_method("compute", JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Int));
    class.push_method("greet", JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Void));
    class.to_bytes().expect("编码 class 失败")
}

/// 构造一个含单个 `class` 入口的 `JAR` 二进制。
fn build_sample_jar_bytes() -> Vec<u8> {
    let mut class = JvmClassFile::new("demo/Sample");
    class.push_method("compute", JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Int));
    let mut package = JvmJarPackage::new("sample.jar");
    package.main_class = Some("demo.Sample".to_string());
    package.push_class(&class).expect("写入 class 失败");
    package.to_bytes().expect("编码 JAR 失败")
}

/// 构造 `spy jvm` 的 `SpyOptions`，便于各 CLI 行为测试按需覆盖字段。
fn build_jvm_spy_opts(input: &str, method: Option<&str>, list: bool, json: bool) -> SpyOptions {
    SpyOptions {
        mode: SpyMode::Jvm(SpyTargetOptions {
            input: Some(input.to_string()),
            func: None,
            method: method.map(String::from),
            offset: None,
            list,
            context: 20,
            target_platform: None,
            json,
            hex: false,
            types: false,
            gc_audit: false,
            glue_audit: false,
        }),
    }
}

/// 将 `class` 字节写入临时 `.class` 文件，返回临时目录与文件路径字符串。
///
/// 返回的 `TempDir` 必须由调用方持有，直到测试断言完成后再释放，以避免文件被提前删除。
fn write_class_fixture() -> (TempDir, String) {
    let temp_dir = tempfile::tempdir().expect("无法创建临时目录");
    let bytes = build_sample_class_bytes();
    let class_path = temp_dir.path().join("Sample.class");
    std::fs::write(&class_path, &bytes).expect("无法写入 Sample.class");
    let path_str = class_path.to_string_lossy().to_string();
    (temp_dir, path_str)
}

/// 将 `JAR` 字节写入临时 `.jar` 文件，返回临时目录与文件路径字符串。
fn write_jar_fixture() -> (TempDir, String) {
    let temp_dir = tempfile::tempdir().expect("无法创建临时目录");
    let bytes = build_sample_jar_bytes();
    let jar_path = temp_dir.path().join("sample.jar");
    std::fs::write(&jar_path, &bytes).expect("无法写入 sample.jar");
    let path_str = jar_path.to_string_lossy().to_string();
    (temp_dir, path_str)
}

#[test]
fn spy_jvm_list_mode_succeeds() {
    let (_temp_dir, path) = write_class_fixture();
    let opts = build_jvm_spy_opts(&path, None, true, false);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "--list 模式应成功：{:?}", result.err());
}

#[test]
fn spy_jvm_method_mode_disassembles_body() {
    let (_temp_dir, path) = write_class_fixture();
    let opts = build_jvm_spy_opts(&path, Some("compute"), false, false);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "--method compute 模式应成功：{:?}", result.err());
}

#[test]
fn spy_jvm_default_mode_acts_as_signature_list() {
    let (_temp_dir, path) = write_class_fixture();
    let opts = build_jvm_spy_opts(&path, None, false, false);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "默认模式（未指定 --method）应输出方法签名：{:?}", result.err());
}

#[test]
fn spy_jvm_json_list_mode_succeeds() {
    let (_temp_dir, path) = write_class_fixture();
    let opts = build_jvm_spy_opts(&path, None, true, true);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "--list --json 模式应成功：{:?}", result.err());
}

#[test]
fn spy_jvm_json_method_mode_succeeds() {
    let (_temp_dir, path) = write_class_fixture();
    let opts = build_jvm_spy_opts(&path, Some("compute"), false, true);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "--method compute --json 模式应成功：{:?}", result.err());
}

#[test]
fn spy_jvm_json_default_mode_succeeds() {
    let (_temp_dir, path) = write_class_fixture();
    let opts = build_jvm_spy_opts(&path, None, false, true);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "默认 --json 模式应输出完整 class JSON：{:?}", result.err());
}

#[test]
fn spy_jvm_jar_list_mode_succeeds() {
    let (_temp_dir, path) = write_jar_fixture();
    let opts = build_jvm_spy_opts(&path, None, true, false);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "JAR --list 模式应成功：{:?}", result.err());
}

#[test]
fn spy_jvm_jar_func_filter_succeeds() {
    let (_temp_dir, path) = write_jar_fixture();
    let mut opts = build_jvm_spy_opts(&path, None, true, false);
    if let SpyMode::Jvm(ref mut target) = opts.mode {
        target.func = Some("demo/Sample".to_string());
    }
    let result = run_spy(&opts);
    assert!(result.is_ok(), "JAR --func 过滤应成功：{:?}", result.err());
}

#[test]
fn spy_jvm_jar_method_mode_succeeds() {
    let (_temp_dir, path) = write_jar_fixture();
    let opts = build_jvm_spy_opts(&path, Some("compute"), false, false);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "JAR --method compute 模式应成功：{:?}", result.err());
}

#[test]
fn spy_jvm_jar_json_mode_succeeds() {
    let (_temp_dir, path) = write_jar_fixture();
    let opts = build_jvm_spy_opts(&path, None, true, true);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "JAR --list --json 模式应成功：{:?}", result.err());
}

#[test]
fn spy_jvm_method_not_found_still_succeeds() {
    let (_temp_dir, path) = write_class_fixture();
    let opts = build_jvm_spy_opts(&path, Some("nonexistent"), false, false);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "未匹配方法名时不应报错：{:?}", result.err());
}
