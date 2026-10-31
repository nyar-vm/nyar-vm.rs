//! C0 配置求值探针：验证「V 源码 → 真实编译 → Node 执行 → 经 JS glue 读回导出返回值」候选路线。
//!
//! 账本（`内部决策/06-当前卡点.md`「Asgard 静态页面交付线」）指定的最早红点：
//! 仓库内不存在现成的「V 求值取回对象值」设施；C0 先验证 Node + Wasm GC 工具链能否
//! 承担 Asgard 配置模块的两阶段求值（设计 15 §5）。本探针走正式交付线：
//! `legion build`（manifest → 规划 → 前端 → canonical → emitter）产出 `artifact: "library"`
//! 的 Node `.mjs` glue + `.wasm`，再用 node 导入 glue 的 `callExport` 读回导出函数返回值。
//!
//! 禁止路线（探针未通过前）：在 Asgard 内写第二套求值器、任何文本解析
//! （字符扫描、VON 转换、按名字提取 `define_config` 大括号内容）。
//!
//! 分层：
//! 1. 标量：`u32` 返回值经 glue 读回整数；
//! 2. 文本：`utf8` 返回值，验证跨边界字符串 codec 或固化缺口；
//! 3. 聚合：结构体返回值，验证对象读回或固化缺口。

use rusty_valkyrie::legion::{
    CanonicalTarget,
    cmds::build::{BuildArgs, run},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

/// 夹具项目源级 `core/primitive` 声明族，与 `tests/legion/support` 同一合同：
/// 数值原始类型名不是语言内建，必须绑定到源码声明身份。
const CORE_PRIMITIVE_DECLARATIONS: &str = concat!(
    "[primitive(\"core::primitive::i8\")] structure i8 { }\n",
    "[primitive(\"core::primitive::i16\")] structure i16 { }\n",
    "[primitive(\"core::primitive::i32\")] structure i32 { }\n",
    "[primitive(\"core::primitive::i64\")] structure i64 { }\n",
    "[primitive(\"core::primitive::i128\")] structure i128 { }\n",
    "[primitive(\"core::primitive::u8\")] structure u8 { }\n",
    "[primitive(\"core::primitive::u16\")] structure u16 { }\n",
    "[primitive(\"core::primitive::u32\")] structure u32 { }\n",
    "[primitive(\"core::primitive::u64\")] structure u64 { }\n",
    "[primitive(\"core::primitive::u128\")] structure u128 { }\n",
    "[primitive(\"core::primitive::isize\")] structure isize { }\n",
    "[primitive(\"core::primitive::usize\")] structure usize { }\n",
    "[primitive(\"core::primitive::f32\")] structure f32 { }\n",
    "[primitive(\"core::primitive::f64\")] structure f64 { }\n",
    "[primitive(\"core::primitive::f128\")] structure f128 { }\n",
    "[primitive(\"core::primitive::byte\")] structure byte { }\n",
    "[primitive(\"core::primitive::sbyte\")] structure sbyte { }\n",
);

/// 一次探针构建的可观察结果：产物目录与 Node 可导入的 glue 入口。
pub(crate) struct ProbeArtifact {
    _temp: tempfile::TempDir,
    pub(crate) output_dir: PathBuf,
    pub(crate) glue_path: PathBuf,
}

/// 构建失败的结构化诊断（保留编译输出原文，失败闭合，不伪造成功）。
pub(crate) struct ProbeFailure {
    pub(crate) message: String,
}

/// 用最小 `legion` 工程（`artifact: "library"` + `target: "node"`）走正式构建链。
///
/// 成功返回落盘产物目录与 `.mjs` glue 路径；失败返回聚合后的结构化诊断文本。
pub(crate) fn build_probe_project(prefix: &str, source: &str) -> Result<ProbeArtifact, ProbeFailure> {
    let temp = tempfile::Builder::new().prefix(prefix).tempdir().expect("探针临时目录");
    let root = temp.path();
    let project_dir = root.join("app");
    let source_dir = project_dir.join("source");
    fs::create_dir_all(&source_dir).expect("创建探针工程源码目录");
    fs::write(root.join("legions.von"), "{\n    name: \"asgard-config-probe\",\n    members: [\"app\"]\n}\n").expect("写入 workspace 清单");
    fs::write(project_dir.join("legion.von"), "{\n    name: \"app\",\n    artifact: \"library\",\n    build: [{ target: \"node\" }]\n}\n")
        .expect("写入工程清单");
    fs::write(source_dir.join("main.v"), source).expect("写入探针配置模块源码");
    fs::write(source_dir.join("core_primitive.v"), CORE_PRIMITIVE_DECLARATIONS).expect("写入 primitive 声明");

    let output_dir = project_dir.join("dist").join("probe");
    let status = run(&BuildArgs {
        project_dir: project_dir.clone(),
        target: CanonicalTarget::parse("node").expect("Node 目标"),
        output_dir: Some(output_dir.clone()),
        debug_artifacts: false,
    });
    let status = match status {
        Ok(code) => code,
        Err(error) => return Err(ProbeFailure { message: format!("legion build 返回错误: {error:?}") }),
    };
    if status != ExitCode::SUCCESS {
        return Err(ProbeFailure { message: format!("legion build 退出码非成功: {status:?}") });
    }

    let glue_path = find_glue(&output_dir)
        .ok_or_else(|| ProbeFailure { message: format!("构建成功但输出目录没有 `.mjs` Node glue: {}", list_dir(&output_dir)) })?;
    Ok(ProbeArtifact { _temp: temp, output_dir, glue_path })
}

fn find_glue(output_dir: &Path) -> Option<PathBuf> {
    fs::read_dir(output_dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("mjs"))
}

fn list_dir(dir: &Path) -> String {
    fs::read_dir(dir)
        .map(|entries| {
            entries.filter_map(|entry| entry.ok()).map(|entry| entry.file_name().to_string_lossy().into_owned()).collect::<Vec<_>>().join(", ")
        })
        .unwrap_or_else(|_| "<目录不存在>".to_string())
}

/// Node 侧经 JS glue 的 `callExport` 读回导出函数返回值。
///
/// 返回 `Ok(stdout)`（glue 正常完成调用）或 `Err(诊断)`（glue/编译器抛出的结构化错误，
/// 含退出码与 stderr，供缺口固化断言使用）。
pub(crate) fn call_export_through_glue(artifact: &ProbeArtifact, export_name: &str, args_json: &str) -> Result<String, String> {
    let runner_path = artifact.output_dir.join("c0_probe_runner.mjs");
    fs::write(
        &runner_path,
        r#"import { pathToFileURL } from "node:url";
const glue = await import(pathToFileURL(process.argv[2]).href);
const value = await glue.callExport(process.argv[3], ...JSON.parse(process.argv[4]));
process.stdout.write(JSON.stringify(value ?? null));
"#,
    )
    .expect("写入 Node 探针 runner");
    let output = Command::new("node")
        .arg(&runner_path)
        .arg(&artifact.glue_path)
        .arg(export_name)
        .arg(args_json)
        .output()
        .expect("Node 必须可用（主动交付线宿主）");
    if !output.status.success() {
        return Err(format!("node callExport 失败 status={:?}\nstderr={}", output.status.code(), String::from_utf8_lossy(&output.stderr)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// 第一层（标量）：最小配置模块的整数返回值必须经真实编译 + Node + JS glue 读回。
#[test]
fn c0_scalar_config_value_roundtrips_through_node_glue() {
    let artifact = build_probe_project(
        "asgard-c0-scalar",
        r#"[export(name: "configure")]
micro configure() -> u32 {
    return 7
}
"#,
    )
    .unwrap_or_else(|failure| panic!("C0 标量层必须走通正式构建链，实际失败：{}", failure.message));

    let value = call_export_through_glue(&artifact, "configure", "[]")
        .unwrap_or_else(|diagnostic| panic!("C0 标量层必须经 glue 读回整数，实际失败：{diagnostic}"));
    assert_eq!(value, "7", "glue 读回的标量配置值必须是导出函数的返回值");
}

/// 第二层（文本）：`utf8` 返回值经正式构建 + Node + JS glue 读回字符串。
///
/// 本轮闭合的两处事实：文本常量的编码身份在 canonical→executable 边界由常量变体恢复，
/// 非入口（库导出）模块同样消费 plan 导入并合成 `const_utf8`；glue 侧 `Utf8` 句柄
/// 经 host-intern 表解码回 JS 字符串，入参字符串同样登记为句柄。
#[test]
fn c0_text_layer_roundtrips_through_node_glue() {
    let artifact = build_probe_project(
        "asgard-c0-text",
        r#"[export(name: "title")]
micro title() -> utf8 {
    return "asgard"
}
"#,
    )
    .unwrap_or_else(|failure| panic!("C0 文本层必须走通正式构建链，实际失败：{}", failure.message));

    let value = call_export_through_glue(&artifact, "title", "[]")
        .unwrap_or_else(|diagnostic| panic!("C0 文本层必须经 glue 读回字符串，实际失败：{diagnostic}"));
    assert_eq!(value, "\"asgard\"", "glue 读回的文本配置值必须是导出函数返回的字符串");
}

/// 第三层（聚合）：结构体返回值当前断在 wasm 预发射名义身份门禁。
///
/// 探针实测（2026-10-31）：最小配置模块返回用户声明结构体时，正式构建链在
/// WASM pre-emission verifier 失败闭合：「携带无身份名义类型 `ConfigArtifact`；
/// declaration / `TypeId` 缺失时不得按短名解析存储类」。这与账本「预编码门禁
/// （`entry.rs validate_storage_identities`）」同源，归前端 MIR SSA 名义身份登记
/// （Lane G2 在途工作面，本探针不触碰其文件）。
/// 即便身份门禁转通，配置对象读回仍有第二道已知缺口：JS glue 的 NamedAggregate
/// demarshal 只支持序列族（length/at），没有按字段访问结构体的合同；
/// `render_glue_entries` 对非序列聚合停在 `pending_export_name`。
/// 本用例固化失败形态：门禁转通后此测试转红，探针须升级为读回断言并暴露
/// 字段访问合同缺口（归后端 glue/ABI 与配置 SDK 所有者）。
#[test]
fn c0_aggregate_layer_fails_closed_on_missing_nominal_identity() {
    let result = build_probe_project(
        "asgard-c0-aggregate",
        r#"structure ConfigArtifact {
    target: i32
    entry: i32
}

[export(name: "configure")]
micro configure() -> ConfigArtifact {
    return ConfigArtifact { target: 1, entry: 2 }
}
"#,
    );
    let failure = match result {
        Err(failure) => failure,
        Ok(_) => panic!("C0 聚合层当前必须失败闭合于名义身份门禁；若已转通，请把本探针升级为 glue 读回断言，并固化字段访问合同缺口"),
    };
    assert!(failure.message.contains("pre-emission verifier"), "聚合层缺口必须来自预发射身份门禁的结构化诊断，实际：{}", failure.message);
    assert!(failure.message.contains("ConfigArtifact"), "诊断必须指明无身份的名义类型，实际：{}", failure.message);
}
