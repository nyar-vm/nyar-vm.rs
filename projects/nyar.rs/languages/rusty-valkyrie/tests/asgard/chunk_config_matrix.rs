//! `artifact.chunks` 严格模型接受/拒绝矩阵的系统性覆盖（设计 15 §6/§6.2）。
//!
//! 与 `config_model.rs` 的既有 14 条互补：那里锁定了总体白名单与少量 chunks 拒绝，
//! 本文件把 `boundaries` / `preload` / `prefetch` / `budgets` 每个字段的接受与拒绝
//! 形态逐格覆盖。注意：模型接受 ≠ 产物消费——目标策略侧的消费（浏览器保留分割计划、
//! 桌面 WebView 跳过分割）见 `chunk_planning.rs`；静态构建当前仍只产出 `index.html`，
//! 该形态由 `static_artifact_tree.rs` 锁定。

use std::{fs, path::Path};

use rusty_valkyrie::asgard::config_model::{ChunkBudgets, ChunkPlan, ConfigOrigin, ConfigValue, ResolvedAsgardConfig, resolve_static_config};
use tempfile::TempDir;

fn obj(fields: Vec<(&str, ConfigValue)>) -> ConfigValue {
    ConfigValue::Object(fields.into_iter().map(|(name, value)| (name.to_owned(), value)).collect())
}

fn text(value: &str) -> ConfigValue {
    ConfigValue::Text(value.to_owned())
}

/// 一条 chunk 边界声明。
fn boundary(fields: Vec<(&str, ConfigValue)>) -> ConfigValue {
    obj(fields)
}

/// `boundaries` 列表。
fn boundaries(items: Vec<ConfigValue>) -> ConfigValue {
    ConfigValue::List(items)
}

/// 只带 `chunks` 的配置值。
fn with_chunks(chunks: ConfigValue) -> ConfigValue {
    obj(vec![("artifact", obj(vec![("entry", text("source/App.w")), ("chunks", chunks)]))])
}

/// `chunks` 对象：只带 `boundaries`。
fn chunks_boundaries(items: Vec<ConfigValue>) -> ConfigValue {
    obj(vec![("boundaries", boundaries(items))])
}

/// `chunks` 对象：只带 `budgets`。
fn chunks_budgets(fields: Vec<(&str, ConfigValue)>) -> ConfigValue {
    obj(vec![("budgets", obj(fields))])
}

/// 构造带入口文件的项目根并校验配置。
fn resolve(temp: &Path, value: ConfigValue) -> Result<ResolvedAsgardConfig, String> {
    let entry = temp.join("source").join("App.w");
    fs::create_dir_all(entry.parent().expect("入口应有父目录")).expect("创建入口目录失败");
    fs::write(&entry, "<widget></widget>\n").expect("写入入口文件失败");
    let origin = ConfigOrigin { config_path: temp.join("asgard.config.v"), project_root: temp.to_path_buf() };
    resolve_static_config(value, origin).map_err(|error| error.to_string())
}

fn expect_error(result: Result<ResolvedAsgardConfig, String>, needle: &str) {
    let actual = result.expect_err(&format!("应拒绝并包含 `{needle}`"));
    assert!(actual.contains(needle), "诊断应包含 `{needle}`，实际：{actual}");
}

#[test]
fn boundaries_accept_preload_and_prefetch_combinations() {
    let temp = TempDir::new().expect("创建临时目录失败");

    // 缺省：两个策略位都是 false，不臆造预加载。
    let only_component = boundary(vec![("component", text("A"))]);
    let config = resolve(temp.path(), with_chunks(chunks_boundaries(vec![only_component]))).expect("最小边界应被接受");
    assert_eq!(config.chunks.boundaries[0].component, "A");
    assert!(!config.chunks.boundaries[0].preload);
    assert!(!config.chunks.boundaries[0].prefetch);

    // preload + prefetch 同真、多条边界按声明顺序保留。
    let both = boundary(vec![("component", text("A")), ("preload", ConfigValue::Bool(true)), ("prefetch", ConfigValue::Bool(true))]);
    let prefetch_only = boundary(vec![("component", text("B")), ("prefetch", ConfigValue::Bool(true))]);
    let config = resolve(temp.path(), with_chunks(chunks_boundaries(vec![both, prefetch_only]))).expect("完整策略位应被接受");
    assert_eq!(config.chunks.boundaries.len(), 2);
    assert!(config.chunks.boundaries[0].preload && config.chunks.boundaries[0].prefetch);
    assert_eq!(config.chunks.boundaries[1].component, "B");
    assert!(!config.chunks.boundaries[1].preload && config.chunks.boundaries[1].prefetch);
}

#[test]
fn boundary_rejects_shape_and_type_violations() {
    let temp = TempDir::new().expect("创建临时目录失败");

    // 边界条目必须是对象。
    expect_error(resolve(temp.path(), with_chunks(chunks_boundaries(vec![text("A")]))), "artifact.chunks.boundaries[0]");

    // 缺少必需字段 component。
    let no_component = boundary(vec![("preload", ConfigValue::Bool(true))]);
    expect_error(resolve(temp.path(), with_chunks(chunks_boundaries(vec![no_component]))), "缺少必需字段 `component`");

    // preload 必须是布尔值。
    let bad_preload = boundary(vec![("component", text("A")), ("preload", text("yes"))]);
    expect_error(resolve(temp.path(), with_chunks(chunks_boundaries(vec![bad_preload]))), "期望布尔值");

    // prefetch 必须是布尔值，诊断带字段路径。
    let bad_prefetch = boundary(vec![("component", text("A")), ("prefetch", ConfigValue::Int(1))]);
    expect_error(resolve(temp.path(), with_chunks(chunks_boundaries(vec![bad_prefetch]))), "artifact.chunks.boundaries[0].prefetch");
}

#[test]
fn budgets_accept_zero_and_reject_type_violations() {
    let temp = TempDir::new().expect("创建临时目录失败");

    // 零预算是合法声明（预算为 0 表示策略上禁止该层体积，判定归未来的构建器）。
    let zero_entry = chunks_budgets(vec![("entry_bytes", ConfigValue::Int(0))]);
    let config = resolve(temp.path(), with_chunks(zero_entry)).expect("零预算应被接受");
    assert_eq!(config.chunks.budgets, ChunkBudgets { entry_bytes: Some(0), chunk_bytes: None });

    // 未声明维度保持 None，不臆造默认预算。
    let empty_budgets = chunks_budgets(vec![]);
    let config = resolve(temp.path(), with_chunks(empty_budgets)).expect("空预算对象应被接受");
    assert_eq!(config.chunks.budgets, ChunkBudgets::default());

    // 未知预算维度拒绝。
    let unknown_dim = chunks_budgets(vec![("warn_bytes", ConfigValue::Int(100))]);
    expect_error(resolve(temp.path(), with_chunks(unknown_dim)), "未知字段 `warn_bytes`");

    // 预算必须是整数：浮点与文本都拒绝。
    let float_budget = chunks_budgets(vec![("chunk_bytes", ConfigValue::Float(1.5))]);
    expect_error(resolve(temp.path(), with_chunks(float_budget)), "期望整数");
    let text_budget = chunks_budgets(vec![("chunk_bytes", text("20kb"))]);
    expect_error(resolve(temp.path(), with_chunks(text_budget)), "artifact.chunks.budgets.chunk_bytes");
}

#[test]
fn chunks_section_rejects_non_object_and_keeps_empty_plan_default() {
    let temp = TempDir::new().expect("创建临时目录失败");

    // chunks 必须是对象。
    expect_error(resolve(temp.path(), with_chunks(ConfigValue::List(vec![]))), "artifact.chunks");

    // 空对象合法：得到不填充任何伪边界的空计划。
    let config = resolve(temp.path(), with_chunks(obj(vec![]))).expect("空 chunks 对象应被接受");
    assert_eq!(config.chunks, ChunkPlan::default());
}
