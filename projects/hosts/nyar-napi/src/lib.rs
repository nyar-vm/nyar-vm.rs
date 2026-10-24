//! Node-API 原生绑定：导出正交、粗粒度的宿主能力（协议、运行时家族、命令准备/执行）。
//!
//! CLI 编排与 argv 解析在 `@nyar/nyar`（TypeScript + `@vmz/commander`），不在此 crate。

#![deny(missing_docs)]

use std::{collections::HashMap, path::Path};

use napi::bindgen_prelude::*;
use napi_derive::napi;
use nyar_runner::{PreparedRuntimeCommand, RuntimeContract, RuntimeFamily};

/// 当前 Node 宿主协议 id（与 JS `HOST_PROTOCOL` 对齐）。
pub const HOST_PROTOCOL: &str = "nyar-host/0";

/// 协议与宿主元信息。
#[napi(object)]
pub struct JsHostInfo {
    /// 宿主协议 id。
    pub host_protocol: String,
    /// 绑定 crate 名。
    pub binding: String,
}

/// 准备运行时命令的输入。
#[napi(object)]
pub struct JsPrepareRuntimeOptions {
    /// 运行时家族 id（`clr` / `jvm` / `node` / `wasi` / `nyar-vm` 等）。
    pub family: String,
    /// 产物路径（artifact / jar / wasm 等）。
    pub artifact: String,
    /// JVM classpath；其他家族可留空。
    pub classpath: Option<String>,
    /// 逻辑入口名。
    pub entry: String,
    /// WASI 家族是否启用 `-S p3`。
    pub wasi_p3: Option<bool>,
}

/// 已展开占位符、可直接执行的运行命令。
#[napi(object)]
pub struct JsPreparedRuntimeCommand {
    /// 运行时家族 id。
    pub family: String,
    /// 可执行文件或解释器。
    pub command: String,
    /// 参数列表。
    pub args: Vec<String>,
}

fn parse_family(value: &str) -> Result<RuntimeFamily> {
    RuntimeFamily::parse(value).ok_or_else(|| Error::from_reason(format!("unsupported runtime family '{}'", value)))
}

fn to_js_prepared(cmd: PreparedRuntimeCommand) -> JsPreparedRuntimeCommand {
    JsPreparedRuntimeCommand { family: cmd.family.as_str().to_string(), command: cmd.command, args: cmd.args }
}

/// 返回宿主协议与绑定元信息。
#[napi]
pub fn get_host_info() -> JsHostInfo {
    JsHostInfo { host_protocol: HOST_PROTOCOL.to_string(), binding: "nyar-napi".to_string() }
}

/// 列出支持的运行时家族 id。
#[napi]
pub fn list_runtime_families() -> Vec<String> {
    [RuntimeFamily::Clr, RuntimeFamily::Jvm, RuntimeFamily::Node, RuntimeFamily::Windows, RuntimeFamily::Wasi, RuntimeFamily::Ovm]
        .into_iter()
        .map(|family| family.as_str().to_string())
        .collect()
}

/// 解析运行时家族 id；非法输入返回 Error。
#[napi]
pub fn parse_runtime_family(value: String) -> Result<String> {
    Ok(parse_family(&value)?.as_str().to_string())
}

/// 按家族模板展开占位符，返回可执行命令（不启动进程）。
#[napi]
pub fn prepare_runtime_command(options: JsPrepareRuntimeOptions) -> Result<JsPreparedRuntimeCommand> {
    let family = parse_family(&options.family)?;
    let contract =
        RuntimeContract { logical_entry: Some(options.entry.as_str()), physical_entry: None, wasi_p3: options.wasi_p3.unwrap_or(false) };
    let template = family.default_template(Some(contract));
    let classpath = options.classpath.as_deref().unwrap_or(".");
    let prepared = template.prepare_command(Path::new(&options.artifact), Path::new(classpath), &options.entry);
    Ok(to_js_prepared(prepared))
}

/// 执行宿主脚本的输入。
#[napi(object)]
pub struct JsHostScriptOptions {
    /// 语言 id（`bash` / `sh` / `lua` / `tcl` 等）。
    pub language: String,
    /// 脚本源码。
    pub source: String,
    /// 脚本路径；仅用于诊断，可留空。
    pub file: Option<String>,
}

/// 宿主脚本执行结果。
#[napi(object)]
pub struct JsHostScriptResult {
    /// 脚本最终值的字符串表示。
    pub value: String,
}

/// 执行宿主脚本源码，返回脚本最终值。
#[napi]
pub fn run_host_script(options: JsHostScriptOptions) -> Result<JsHostScriptResult> {
    let runner = legend_vm::LegacyVmRunner::new();
    let mut env = HashMap::new();
    let value = runner
        .run(&options.language, &options.source, &mut env)
        .map_err(|error| Error::from_reason(error.to_string()))?;
    Ok(JsHostScriptResult { value: value.to_string_value() })
}

/// 在指定工作目录执行已准备好的命令并返回退出码。
#[napi]
pub fn run_prepared_command(prepared: JsPreparedRuntimeCommand, cwd: Option<String>) -> Result<i32> {
    let family = parse_family(&prepared.family)?;
    let cmd = PreparedRuntimeCommand { family, command: prepared.command, args: prepared.args };
    let cwd = cwd.as_deref().map(Path::new).unwrap_or_else(|| Path::new("."));
    Ok(cmd.run_in(cwd).map_err(|err| Error::from_reason(err.to_string()))?)
}
