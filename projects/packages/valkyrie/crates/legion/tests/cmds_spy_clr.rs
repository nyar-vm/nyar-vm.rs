//! `legion spy clr` 子命令的集成测试。
//!
//! 覆盖两层：
//! - `MsilParser` 文本解析的单元行为（方法签名抽取、空输入、不闭合块）。
//! - `spy clr <file>.msil` 的 CLI 行为（`--list` / `--method` / `--json`），通过 `run_spy` 验证。
#![cfg(feature = "format-spy")]

use acorn_pe::msil::MsilParser;
use legion::{SpyMode, SpyOptions, SpyTargetOptions, run_spy};
use tempfile::TempDir;

const SAMPLE_MSIL: &str = r#".assembly extern mscorlib {}
.assembly hello {}

.class public auto ansi beforefieldinit Hello
       extends [mscorlib]System.Object
{
  .method public hidebysig static int32 Add(int32 a, int32 b) cil managed
  {
    .maxstack  2
    ldarg.0
    ldarg.1
    add
    ret
  }

  .method public hidebysig static void Main() cil managed
  {
    .entrypoint
    .maxstack  1
    ldc.i4.1
    ret
  }
}
"#;

#[test]
fn parses_method_signatures() {
    let methods = MsilParser::parse_methods(SAMPLE_MSIL);

    assert_eq!(methods.len(), 2, "应解析出 2 个方法");
    assert_eq!(methods[0].name, "Add");
    assert_eq!(methods[1].name, "Main");
}

#[test]
fn empty_input_produces_empty_list() {
    let methods = MsilParser::parse_methods("");
    assert!(methods.is_empty());
}

#[test]
fn unbalanced_block_is_captured_to_eof() {
    let methods = MsilParser::parse_methods(
        r#".method public void Foo() cil managed
{
  ret
"#,
    );
    assert_eq!(methods.len(), 1, "不闭合的方法仍应被收录");
    assert!(!methods[0].body.is_empty());
}

// ========== P11: CLI 行为测试 ==========

/// 构造 `spy clr` 的 `SpyOptions`，便于各 CLI 行为测试按需覆盖字段。
fn build_clr_spy_opts(input: &str, method: Option<&str>, list: bool, json: bool) -> SpyOptions {
    SpyOptions {
        mode: SpyMode::Clr(SpyTargetOptions {
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

/// 将 MSIL 文本写入临时 `.msil` 文件，返回临时目录与文件路径字符串。
///
/// 返回的 `TempDir` 必须由调用方持有，直到测试断言完成后再释放，以避免文件被提前删除。
fn write_msil_fixture(content: &str) -> (TempDir, String) {
    let temp_dir = tempfile::tempdir().expect("无法创建临时目录");
    let msil_path = temp_dir.path().join("sample.msil");
    std::fs::write(&msil_path, content).expect("无法写入 sample.msil");
    let path_str = msil_path.to_string_lossy().to_string();
    (temp_dir, path_str)
}

#[test]
fn spy_clr_list_mode_succeeds() {
    let (_temp_dir, path) = write_msil_fixture(SAMPLE_MSIL);
    let opts = build_clr_spy_opts(&path, None, true, false);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "--list 模式应成功：{:?}", result.err());
}

#[test]
fn spy_clr_method_mode_disassembles_body() {
    let (_temp_dir, path) = write_msil_fixture(SAMPLE_MSIL);
    let opts = build_clr_spy_opts(&path, Some("Add"), false, false);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "--method Add 模式应成功：{:?}", result.err());
}

#[test]
fn spy_clr_default_mode_acts_as_list() {
    // 未指定 --method 时，--list 为隐含默认行为（clr.rs:228 effective_list）。
    let (_temp_dir, path) = write_msil_fixture(SAMPLE_MSIL);
    let opts = build_clr_spy_opts(&path, None, false, false);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "默认模式（未指定 --method）应等价 --list：{:?}", result.err());
}

#[test]
fn spy_clr_json_list_mode_succeeds() {
    let (_temp_dir, path) = write_msil_fixture(SAMPLE_MSIL);
    let opts = build_clr_spy_opts(&path, None, true, true);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "--list --json 模式应成功：{:?}", result.err());
}

#[test]
fn spy_clr_json_method_mode_succeeds() {
    let (_temp_dir, path) = write_msil_fixture(SAMPLE_MSIL);
    let opts = build_clr_spy_opts(&path, Some("Main"), false, true);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "--method Main --json 模式应成功：{:?}", result.err());
}
