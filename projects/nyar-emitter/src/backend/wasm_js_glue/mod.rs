//! `WASM + Node JS glue` 后端：Node 专用 JS launcher 生成。
//!
//! 仅负责为 `wasm32` + Node 运行方式生成 `.mjs` 启动壳。
//! 不承载 `.wasi` / `wasmtime` / 任何 WASI preview 模块语义，
//! 也不依赖 `backend::wasi` 子树的任何符号。

use std::{fs, path::Path};

use miette::{IntoDiagnostic, Result, WrapErr};
use nyar::{
    abstractions::ArtifactFormat,
    packaging::{ArtifactDescriptor, TargetLane},
};
use std_data::binary::wasm::WasmBinaryModule;

use crate::backend::binding_builders::{BindingGenerationContext, HostBindingBuilder};

/// `WASM + JS glue` 宿主绑定生成器。
pub(crate) struct JsGlueBindingBuilder;

impl HostBindingBuilder for JsGlueBindingBuilder {
    fn build(&self, context: &BindingGenerationContext<'_>) -> Result<Vec<ArtifactDescriptor>> {
        let launcher_stem = context.artifact_name;
        let launcher_path = context.output_dir.join(format!("{launcher_stem}.mjs"));
        let wasm_path = context.output_dir.join(format!("{launcher_stem}.wasm"));
        let utf8_literals = read_wasm_utf8_literals(&wasm_path).unwrap_or_default();
        let launcher = build_node_launcher(launcher_stem, context.imports, &utf8_literals);
        fs::write(&launcher_path, launcher).into_diagnostic().wrap_err_with(|| format!("写入 Node 启动壳失败：{}", launcher_path.display()))?;

        Ok(vec![ArtifactDescriptor {
            name: format!("{}.launcher", context.artifact_name),
            kind: nyar::ArtifactKind::AssemblyListing,
            format: ArtifactFormat::RawBinary,
            target: context.target.clone(),
            lane: TargetLane::Wasm,
        }])
    }
}

/// 为指定导入字段生成 `JS` 实现表达式。
///
/// 仅消费 `WASM` 模块真实声明的导入字段，不注入伪导入。
/// `cli_get_*` 系列字段返回安全默认值，不依赖任何 `CLI` 状态。
fn js_import_impl(field: &str) -> String {
    match field {
        "get_input" => "() => input_value".to_string(),
        "read_source_byte" => "() => { if (source_pos < source_bytes.length) { return source_bytes[source_pos++]; } return -1; }".to_string(),
        "emit_byte" => "(b) => { output_bytes.push(b & 0xFF); }".to_string(),
        "emit_i32" => "(v) => { output_bytes.push(v & 0xFF, (v >> 8) & 0xFF, (v >> 16) & 0xFF, (v >> 24) & 0xFF); }".to_string(),
        "add" => "(a, b) => (a + b) | 0".to_string(),
        "sub" => "(a, b) => (a - b) | 0".to_string(),
        "mul" => "(a, b) => (a * b) | 0".to_string(),
        "lt" => "(a, b) => a < b ? 1 : 0".to_string(),
        "gt" => "(a, b) => a > b ? 1 : 0".to_string(),
        "le" => "(a, b) => a <= b ? 1 : 0".to_string(),
        "ge" => "(a, b) => a >= b ? 1 : 0".to_string(),
        "eq" => "(a, b) => a === b ? 1 : 0".to_string(),
        "ne" => "(a, b) => a !== b ? 1 : 0".to_string(),
        "cli_get_project" => "() => hostIntern(cli_project)".to_string(),
        "cli_get_target" => "() => hostIntern(cli_target)".to_string(),
        "cli_get_output" => "() => hostIntern(cli_output)".to_string(),
        "cli_get_verbose" => "() => cli_verbose ? 1 : 0".to_string(),
        "get_current_directory" => "() => hostIntern(cwd())".to_string(),
        "file_exists" => "(path) => hostFileExists(path)".to_string(),
        "directory_exists" => "(path) => hostDirectoryExists(path)".to_string(),
        "create_directory" => "(path) => hostCreateDirectory(path)".to_string(),
        "read_file_text" => "(path) => hostReadFileText(path)".to_string(),
        "write_file_text" => "(path, content) => hostWriteFileText(path, content)".to_string(),
        "get_files" => "(path, pattern, recursive) => hostGetFiles(path, pattern, recursive)".to_string(),
        "const_utf8" => "(index) => hostIntern(hostConstUtf8(index))".to_string(),
        "utf8_concat" => "(a, b) => hostUtf8Concat(a, b)".to_string(),
        "utf8_length" => "(s) => hostUtf8Length(s)".to_string(),
        "utf8_trim" => "(s) => hostIntern(hostUtf8Trim(s))".to_string(),
        "utf8_to_lower" => "(s) => hostIntern(hostUtf8ToLower(s))".to_string(),
        "utf8_to_upper" => "(s) => hostIntern(hostUtf8ToUpper(s))".to_string(),
        "utf8_replace" => "(s, oldValue, newValue) => hostIntern(hostUtf8Replace(s, oldValue, newValue))".to_string(),
        "utf8_starts_with" => "(s, prefix) => hostUtf8StartsWith(s, prefix)".to_string(),
        "utf8_ends_with" => "(s, suffix) => hostUtf8EndsWith(s, suffix)".to_string(),
        "utf8_contains" => "(s, other) => hostUtf8Contains(s, other)".to_string(),
        "utf8_equals" => "(a, b) => hostUtf8Equals(a, b)".to_string(),
        "utf8_index_of" => "(s, other) => hostUtf8IndexOf(s, other)".to_string(),
        "utf8_slice" => "(s, start, count) => hostIntern(hostUtf8Slice(s, start, count))".to_string(),
        _ => "() => 0".to_string(),
    }
}

/// 将字段名格式化为合法的 `JS` 对象键。
fn js_object_key(field: &str) -> String {
    if field.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
        && !field.chars().next().is_some_and(|ch| ch.is_ascii_digit())
    {
        field.to_string()
    }
    else {
        format!("\"{}\"", field.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// 收集去重后的导入绑定。
///
/// 仅消费源码声明的 `[wasm(...)]` / contract 解析结果，
/// 不塞默认导入，无产品名耦合。
fn collect_import_bindings(imports: &[(String, String)]) -> Vec<(String, String)> {
    let mut bindings = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (module, field) in imports {
        let key = (module.clone(), field.clone());
        if seen.insert(key.clone()) {
            bindings.push(key);
        }
    }
    bindings
}

/// 将导入绑定渲染为 `JS` 对象字面量字符串。
fn render_import_object_literal(imports: &[(String, String)]) -> String {
    let bindings = collect_import_bindings(imports);
    let mut by_module: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for (module, field) in bindings {
        by_module.entry(module).or_default().push(field);
    }
    let modules: Vec<String> = by_module
        .iter()
        .map(|(module, fields)| {
            let entries: Vec<String> = fields.iter().map(|field| format!("    {}: {}", js_object_key(field), js_import_impl(field))).collect();
            format!("  {}: {{\n{}\n  }}", js_object_key(module), entries.join(",\n"))
        })
        .collect();
    format!("{{\n{}\n}}", modules.join(",\n"))
}

/// 构建 `Node` 启动壳（`.mjs`）。
///
/// 启动壳只做三件事：读取 `WASM` 字节码、按声明的导入构造 `importObject`、
/// 调用 `exports.main ?? exports._start`。不输出 banner、不解析 CLI 子命令、
/// 不注入伪导入，确保非 CLI 产物的 `stdout` 不被污染。
fn build_node_launcher(artifact_name: &str, imports: &[(String, String)], utf8_literals: &[String]) -> String {
    let utf8_literals_json = serde_json::to_string(utf8_literals).unwrap_or_else(|_| "[]".to_string());
    let has_imports = !imports.is_empty();
    let has_read_source = imports.iter().any(|(_, field)| field == "read_source_byte");
    let smoke_import_object_literal = render_import_object_literal(imports);
    let cli_mode = imports.iter().any(|(_, field)| field == "cli_get_project")
        && imports.iter().any(|(_, field)| field == "cli_get_target")
        && imports.iter().any(|(_, field)| field == "cli_get_output");

    let smoke_import_object = if has_imports {
        format!(
            r#"const importObject = {import_object};
const instance = await wasmResolveInstance(wasmBytes, importObject);"#,
            import_object = smoke_import_object_literal
        )
    }
    else {
        r#"const instance = await wasmResolveInstance(wasmBytes, {});"#.to_string()
    };

    let smoke_output_logic = if has_imports {
        r#"
if (output_path && output_bytes.length > 0) {
    writeFileSync(output_path, Buffer.from(output_bytes));
    process.exit(0);
} else if (output_bytes.length > 0) {
    process.stdout.write(Buffer.from(output_bytes));
    if (typeof result === "number") {
        process.exit(result);
    }
} else if (typeof result === "number") {
    process.exit(result);
}"#
    }
    else {
        r#"
if (typeof result === "number") {
    process.exit(result);
}"#
    };

    let smoke_arg_parsing = if has_imports {
        if has_read_source {
            r#"const source_path = process.argv[2] || null;
const output_path = process.argv[3] || null;
let source_bytes = Buffer.alloc(0);
let source_pos = 0;
if (source_path) {
    try {
        source_bytes = readFileSync(source_path);
    } catch (e) {}
}
const input_value = 0;
const output_bytes = [];
"#
        }
        else {
            r#"const input_value = process.argv[2] ? parseInt(process.argv[2], 10) : 0;
const output_path = process.argv[3] || null;
const output_bytes = [];
"#
        }
    }
    else {
        ""
    };

    let cli_dispatch = if cli_mode {
        // 空 argv 仅在存在 `help` 导出时走帮助；否则回退到 `main`，避免非完整 CLI 模块被误伤。
        r#"
const args = process.argv.slice(2);
const command = args[0] ?? "";
if (command === "--version" || command === "-V") {
    if (typeof exports.version !== "function") throw new Error("wasm module does not export version");
    process.stdout.write(hostResolve(exports.version()) + "\n");
    process.exit(0);
}
if (command === "--help" || command === "-h") {
    if (typeof exports.help !== "function") throw new Error("wasm module does not export help");
    const code = exports.help();
    if (typeof code === "number" && code !== 0) process.exit(code);
    process.exit(0);
}
if (command === "" && typeof exports.help === "function") {
    const code = exports.help();
    if (typeof code === "number" && code !== 0) process.exit(code);
    process.exit(0);
}
if (command === "build") {
    const positional = [];
    for (let i = 1; i < args.length; i++) {
        if (args[i] === "--target") { cli_target = args[++i] ?? cli_target; continue; }
        if (args[i] === "-o" || args[i] === "--output") { cli_output = args[++i] ?? cli_output; continue; }
        if (args[i] === "--verbose" || args[i] === "-v") { cli_verbose = true; continue; }
        if (!args[i].startsWith("-")) positional.push(args[i]);
    }
    cli_project = positional[0] ?? "";
    if (!cli_project) throw new Error("build requires <project-dir>");
    if (!cli_output) cli_output = pathJoin(cwd(), "dist", "build");
    if (typeof exports.build !== "function") throw new Error("wasm module does not export build");
    const code = exports.build();
    if (typeof code === "number") process.exit(code);
    process.exit(0);
}
"#
    }
    else {
        ""
    };

    format!(
        r#"import {{ readFileSync, writeFileSync, readdirSync, mkdirSync, accessSync, constants as fsConstants }} from "node:fs";
import {{ resolve as pathResolve, join as pathJoin }} from "node:path";
import {{ cwd }} from "node:process";

const UTF8_LITERALS = {utf8_literals_json};
const hostValues = [""];
const hostValueIds = new Map([["", 0]]);
let cli_project = "";
let cli_target = "node";
let cli_output = "";
let cli_verbose = false;

function hostIntern(value) {{
    const text = String(value ?? "");
    const known = hostValueIds.get(text);
    if (known !== undefined) return known;
    const handle = hostValues.length;
    hostValues.push(text);
    hostValueIds.set(text, handle);
    return handle;
}}

function hostResolve(handle) {{
    return Number.isInteger(handle) && handle >= 0 && handle < hostValues.length ? hostValues[handle] : "";
}}

function hostConstUtf8(index) {{
    return UTF8_LITERALS[index] ?? "";
}}

function hostPathFromRef(pathRef) {{
    return typeof pathRef === "string" ? pathRef : hostResolve(pathRef);
}}

function hostUtf8Concat(a, b) {{
    return hostIntern(hostPathFromRef(a) + hostPathFromRef(b));
}}

// env.utf8_* index contract (matches std Utf8Text): Unicode **scalars** via [...s],
// never JS string.length / indexOf UTF-16 code units. Host JS strings are UTF-16 storage only.
function hostUtf8Scalars(s) {{
    return [...hostPathFromRef(s)];
}}

function hostUtf8Length(s) {{
    return hostUtf8Scalars(s).length;
}}

function hostUtf8Trim(s) {{
    return hostPathFromRef(s).trim();
}}

function hostUtf8ToLower(s) {{
    return hostPathFromRef(s).toLowerCase();
}}

function hostUtf8ToUpper(s) {{
    return hostPathFromRef(s).toUpperCase();
}}

function hostUtf8Replace(s, oldValue, newValue) {{
    const text = hostPathFromRef(s);
    const needle = hostPathFromRef(oldValue);
    const replacement = hostPathFromRef(newValue);
    if (hostUtf8Scalars(needle).length === 0) return text;
    return text.split(needle).join(replacement);
}}

function hostUtf8StartsWith(s, prefix) {{
    return hostPathFromRef(s).startsWith(hostPathFromRef(prefix)) ? 1 : 0;
}}

function hostUtf8EndsWith(s, suffix) {{
    return hostPathFromRef(s).endsWith(hostPathFromRef(suffix)) ? 1 : 0;
}}

function hostUtf8Contains(s, other) {{
    return hostPathFromRef(s).includes(hostPathFromRef(other)) ? 1 : 0;
}}

function hostUtf8Equals(a, b) {{
    return hostPathFromRef(a) === hostPathFromRef(b) ? 1 : 0;
}}

function hostUtf8IndexOf(s, other) {{
    const chars = hostUtf8Scalars(s);
    const needle = hostUtf8Scalars(other);
    if (needle.length === 0) return 0;
    if (needle.length > chars.length) return -1;
    for (let i = 0; i <= chars.length - needle.length; i++) {{
        let ok = true;
        for (let j = 0; j < needle.length; j++) {{
            if (chars[i + j] !== needle[j]) {{ ok = false; break; }}
        }}
        if (ok) return i;
    }}
    return -1;
}}

function hostUtf8Slice(s, start, count) {{
    const chars = hostUtf8Scalars(s);
    const from = Math.max(0, start | 0);
    const len = Math.max(0, count | 0);
    return chars.slice(from, from + len).join("");
}}

async function wasmResolveInstance(wasmBytes, importObject) {{
    const result = await WebAssembly.instantiate(wasmBytes, importObject);
    return result instanceof WebAssembly.Instance ? result : result.instance;
}}

function hostGetCurrentDirectory() {{
    return cwd();
}}

function hostFileExists(pathRef) {{
    const target = hostPathFromRef(pathRef);
    try {{
        accessSync(target, fsConstants.F_OK);
        return 1;
    }} catch {{
        return 0;
    }}
}}

function hostDirectoryExists(pathRef) {{
    const target = hostPathFromRef(pathRef);
    try {{
        accessSync(target, fsConstants.F_OK);
        return 1;
    }} catch {{
        return 0;
    }}
}}

function hostCreateDirectory(pathRef) {{
    const target = hostPathFromRef(pathRef);
    mkdirSync(target, {{ recursive: true }});
}}

function hostReadFileText(pathRef) {{
    const target = hostPathFromRef(pathRef);
    return hostIntern(readFileSync(target, "utf8"));
}}

function hostWriteFileText(pathRef, contentRef) {{
    const target = hostPathFromRef(pathRef);
    const content = typeof contentRef === "string" ? contentRef : String(contentRef ?? "");
    writeFileSync(target, content, "utf8");
    return 1;
}}

function hostGlobMatch(filePath, pattern) {{
    if (!pattern || pattern === "*") {{
        return true;
    }}
    const normalized = filePath.replace(/\\/g, "/");
    const pat = pattern.replace(/\\/g, "/");
    if (pat.startsWith("*.")) {{
        return normalized.endsWith(pat.slice(1));
    }}
    if (pat.includes("*")) {{
        const escaped = pat.replace(/[.+?^${{}}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*");
        return new RegExp(escaped + "$").test(normalized);
    }}
    return normalized.includes(pat);
}}

function hostEntryFullPath(target, entry) {{
    if (entry.parentPath && entry.parentPath.length > 0) {{
        return pathResolve(entry.parentPath, entry.name);
    }}
    return pathResolve(target, entry.name);
}}

function hostGetFiles(pathRef, patternRef, recursive) {{
    const target = hostPathFromRef(pathRef);
    const pattern = hostPathFromRef(patternRef);
    const entries = readdirSync(target, {{ withFileTypes: true, recursive: recursive !== 0 }});
    const files = [];
    for (const entry of entries) {{
        if (entry.isFile()) {{
            const full = hostEntryFullPath(target, entry);
            if (hostGlobMatch(full, pattern)) {{
                files.push(full);
            }}
        }}
    }}
    return hostIntern(JSON.stringify(files));
}}

{arg_parsing}const wasmBytes = readFileSync(new URL("./{name}.wasm", import.meta.url));
{import_object}
const exports = instance.exports;
{cli_dispatch}
const entry = exports.main ?? exports._start;
let result;
if (typeof entry === "function") {{
    result = entry();
}}
{output_logic}"#,
        name = artifact_name,
        arg_parsing = smoke_arg_parsing,
        import_object = smoke_import_object,
        output_logic = smoke_output_logic,
        cli_dispatch = cli_dispatch,
        utf8_literals_json = utf8_literals_json,
    )
}

/// 从 `WASM` 自定义段读取 `UTF-8` 字面量表。
///
/// 读取名为 `nyar.strings` 的自定义段（由 `lowering` 后端写入），
/// 使用发射器命名空间而非产品名，作为跨后端的线格式契约。
fn read_wasm_utf8_literals(wasm_path: &Path) -> Result<Vec<String>> {
    let bytes = fs::read(wasm_path).into_diagnostic().wrap_err_with(|| format!("读取 WASM 失败：{}", wasm_path.display()))?;
    let module = WasmBinaryModule::from_bytes(&bytes).map_err(|error| miette::miette!("WASM 解析失败：{error}"))?;
    for section in &module.sections {
        if section.name.as_deref() == Some("nyar.strings") {
            let payload: serde_json::Value =
                serde_json::from_slice(&section.bytes).into_diagnostic().wrap_err("解析 nyar.strings 自定义段失败")?;
            if let Some(values) = payload.get("strings").and_then(|value| value.as_array()) {
                return Ok(values.iter().filter_map(|value| value.as_str().map(str::to_string)).collect());
            }
        }
    }
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 通过 `concat!` 拼接的被禁模式常量，避免在源码中出现完整字面量。
    /// 这些模式用于验证生成的启动壳不包含上层专用协议污染。
    const BANNED_BANNER: &str = concat!("Valkyrie", " ", "legion", " ", "CLI");
    const BANNED_BOOTSTRAP_ENV: &str = concat!("LEGION", "_", "BOOTSTRAP", "_", "HOST");
    const BANNED_HOST_ENV: &str = concat!("LEGION", "_", "HOST");
    const BANNED_MANIFEST_PATH: &str = concat!("legion", ".", "von");
    const BANNED_PROJECT_NAME: &str = concat!("legion", ".", "tools");
    const BANNED_HOST_COMPILE_FROM_PLAN_RUST: &str = concat!("host", "_", "legion", "_", "compile", "_", "from", "_", "plan");
    const BANNED_HOST_COMPILE_FROM_PLAN_JS: &str = concat!("host", "Legion", "Compile", "From", "Plan");

    /// 验证 `collect_import_bindings` 仅消费源码声明的 imports，去重且不注入伪导入。
    #[test]
    fn collect_import_bindings_consumes_only_declared_imports() {
        let imports = vec![
            ("env".to_string(), "emit_byte".to_string()),
            ("env".to_string(), "emit_byte".to_string()),
            ("env".to_string(), "read_source_byte".to_string()),
        ];
        let bindings = collect_import_bindings(&imports);
        assert_eq!(bindings.len(), 2, "应去重重复的导入字段");
        assert!(bindings.contains(&("env".to_string(), "emit_byte".to_string())));
        assert!(bindings.contains(&("env".to_string(), "read_source_byte".to_string())));
        assert!(!bindings.iter().any(|(_, field)| field == BANNED_HOST_COMPILE_FROM_PLAN_RUST), "不应注入伪导入");
    }

    /// 验证 `collect_import_bindings` 对空导入列表返回空绑定，不塞默认 imports。
    #[test]
    fn collect_import_bindings_empty_input_yields_no_default_imports() {
        let bindings = collect_import_bindings(&[]);
        assert!(bindings.is_empty(), "空导入不应产生任何默认绑定");
    }

    /// 验证 `build_node_launcher` 生成的启动壳遵循 `logical_entry` 契约：
    /// 优先 `exports.main`，回退 `exports._start`。
    #[test]
    fn build_node_launcher_honors_logical_entry_contract() {
        let imports: Vec<(String, String)> = vec![];
        let launcher = build_node_launcher("demo", &imports, &[]);
        assert!(launcher.contains("exports.main ?? exports._start"), "启动壳必须按 logical_entry 契约选择入口");
    }

    #[test]
    fn build_node_launcher_dispatches_build_with_requested_project_target_and_output() {
        let imports = vec![
            ("env".to_string(), "cli_get_project".to_string()),
            ("env".to_string(), "cli_get_target".to_string()),
            ("env".to_string(), "cli_get_output".to_string()),
            ("env".to_string(), "cli_get_verbose".to_string()),
        ];
        let launcher = build_node_launcher("legion", &imports, &[]);

        assert!(launcher.contains("command === \"build\""));
        assert!(launcher.contains("cli_project = positional[0]"));
        assert!(launcher.contains("cli_target = args[++i]"));
        assert!(launcher.contains("cli_output = args[++i]"));
        assert!(launcher.contains("exports.build"));
        assert!(launcher.contains("exports.version"));
        assert!(launcher.contains("exports.help"));
        assert!(launcher.contains("command === \"\" && typeof exports.help === \"function\""), "空 argv 仅在存在 help 时走帮助");
        assert!(!launcher.contains(BANNED_HOST_COMPILE_FROM_PLAN_JS));
    }

    #[test]
    fn build_node_launcher_without_cli_imports_skips_cli_dispatch() {
        let imports = vec![("env".to_string(), "emit_byte".to_string())];
        let launcher = build_node_launcher("demo", &imports, &[]);
        assert!(!launcher.contains("command === \"build\""), "非 CLI 导入不应启用 build 分派");
        assert!(!launcher.contains("exports.help"), "非 CLI 导入不应要求 help 导出");
        assert!(launcher.contains("exports.main ?? exports._start"));
    }

    /// 验证单入口 stdout 契约：当 imports 包含 `emit_byte` 时，
    /// 启动壳生成 `process.stdout.write` 输出路径。
    #[test]
    fn build_node_launcher_single_entry_stdout_contract() {
        let imports = vec![("env".to_string(), "emit_byte".to_string())];
        let launcher = build_node_launcher("demo", &imports, &[]);
        assert!(launcher.contains("process.stdout.write"), "单入口 stdout 契约：应提供 stdout 输出路径");
        assert!(launcher.contains("output_bytes"), "单入口 stdout 契约：应使用 output_bytes 缓冲区");
    }

    /// 验证非 CLI 程序的 `stdout` 不被上层专用协议污染：
    /// 启动壳不输出 banner、不读取被禁环境变量、不硬编码清单路径或 project_name。
    #[test]
    fn build_node_launcher_does_not_pollute_non_cli_stdout_with_banner() {
        let imports = vec![("env".to_string(), "emit_byte".to_string())];
        let launcher = build_node_launcher("demo", &imports, &[]);
        assert!(!launcher.contains(BANNED_BANNER), "不应输出 banner");
        assert!(!launcher.contains(BANNED_BOOTSTRAP_ENV), "不应读取 bootstrap host 环境变量");
        assert!(!launcher.contains(BANNED_HOST_ENV), "不应读取 host 环境变量");
        assert!(!launcher.contains(BANNED_MANIFEST_PATH), "不应硬编码清单路径");
        assert!(!launcher.contains(BANNED_PROJECT_NAME), "不应硬编码 project_name");
        assert!(!launcher.contains(BANNED_HOST_COMPILE_FROM_PLAN_JS), "不应包含上层专用编译协议实现");
    }

    /// 验证无 imports 时启动壳仍可正常实例化 `WASM` 模块，且使用空 `importObject`。
    #[test]
    fn build_node_launcher_no_imports_uses_empty_import_object() {
        let imports: Vec<(String, String)> = vec![];
        let launcher = build_node_launcher("demo", &imports, &[]);
        assert!(launcher.contains("WebAssembly.instantiate"), "应实例化 WASM 模块");
        assert!(launcher.contains("wasmResolveInstance(wasmBytes, {})"), "无 imports 时应使用空 importObject");
    }

    /// 验证 `render_import_object_literal` 仅渲染源码声明的 imports，不追加伪导入字段。
    #[test]
    fn render_import_object_literal_excludes_synthetic_imports() {
        let imports = vec![("env".to_string(), "emit_byte".to_string())];
        let literal = render_import_object_literal(&imports);
        assert!(literal.contains("emit_byte"), "应渲染声明的 emit_byte 字段");
        assert!(!literal.contains(BANNED_HOST_COMPILE_FROM_PLAN_RUST), "不应追加伪导入字段");
        assert!(!literal.contains(BANNED_HOST_COMPILE_FROM_PLAN_JS), "不应追加伪导入字段的 JS 实现");
    }
}
