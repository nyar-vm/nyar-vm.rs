//! `WASM + Node JS glue` 后端：Node 专用 JS launcher 生成。
//!
//! 仅负责为 `wasm32` + Node 运行方式生成 `.mjs` 启动壳。
//! 不承载 `.wasi` / `wasmtime` / 任何 WASI preview 模块语义，
//! 也不依赖 `backend::wasi` 子树的任何符号。

use std::{collections::BTreeSet, fs, path::Path};

use crate::{EmitterError, Result};
use acorn_wasm::{WasmBinaryModule, WasmExternalKind, parse_export_section};
use nyar::{
    abstractions::ArtifactFormat,
    packaging::{ArtifactDescriptor, TargetLane},
};

use crate::{
    backend::binding_builders::{BindingGenerationContext, HostBindingBuilder},
    nyar_backend_wasi::WasmPackageKind,
};

/// `WASM + JS glue` 宿主绑定生成器。
pub(crate) struct JsGlueBindingBuilder;

impl HostBindingBuilder for JsGlueBindingBuilder {
    fn build(&self, context: &BindingGenerationContext<'_>) -> Result<Vec<ArtifactDescriptor>> {
        let launcher_stem = context.artifact_name;
        let launcher_path = context.output_dir.join(format!("{launcher_stem}.mjs"));
        let wasm_path = context.output_dir.join(format!("{launcher_stem}.wasm"));
        let utf8_literals = read_wasm_utf8_literals(&wasm_path).unwrap_or_default();
        let library_mode = match context.wasm_package_kind {
            WasmPackageKind::Library => true,
            WasmPackageKind::Binary => false,
        };
        let library_invoke = if library_mode {
            validate_library_invoke_exports(&wasm_path)?;
            read_library_invoke_spec(&wasm_path)
        }
        else {
            None
        };
        let launcher = build_node_launcher(launcher_stem, context.imports, &utf8_literals, library_mode, library_invoke.as_deref());
        fs::write(&launcher_path, launcher).map_err(|e| format!("写入 Node 启动壳失败：{}: {e}", launcher_path.display()))?;

        Ok(vec![ArtifactDescriptor {
            name: format!("{}.launcher", context.artifact_name),
            path: format!("{}.mjs", context.artifact_name),
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
        "random" => "(seed) => (Math.imul(seed ^ 0x9e3779b9, 0x85ebca6b) >>> 0) | 0".to_string(),
        "fetch" => "(path) => (path ^ 0x5f3759df) | 0".to_string(),
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
#[doc(hidden)]
pub fn collect_import_bindings(imports: &[(String, String)]) -> Vec<(String, String)> {
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

/// 将导入绑定渲染为 `JS` 对象字面量字符串（集成测试专用）。
#[doc(hidden)]
pub fn render_import_object_literal(imports: &[(String, String)]) -> String {
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

fn is_library_wasm_module(wasm_path: &Path) -> bool {
    let Ok(bytes) = fs::read(wasm_path)
    else {
        return false;
    };
    let Ok(module) = WasmBinaryModule::from_bytes(&bytes)
    else {
        return false;
    };
    let exports = parse_export_section(&module);
    let has_entry = exports.iter().any(|item| item.name == "main" || item.name == "_start");
    let has_named_function = exports.iter().any(|item| item.kind == WasmExternalKind::Func && item.name != "main" && item.name != "_start");
    has_named_function && !has_entry
}

fn read_library_invoke_spec(wasm_path: &Path) -> Option<String> {
    let bytes = fs::read(wasm_path).ok()?;
    let module = WasmBinaryModule::from_bytes(&bytes).ok()?;
    for section in &module.sections {
        if section.name.as_deref() == Some("nyar.library_invoke") {
            return Some(String::from_utf8_lossy(&section.bytes).to_string());
        }
    }
    None
}

/// 校验 `nyar.library_invoke` 的 glue 合同引用真实存在的 Wasm export。
///
/// 选择在生成 JS glue 的最早位置校验：lowering 阶段的
/// `append_library_mode_glue` 只有 plan 的 `wasm_export_names()`，看不到最终
/// export section；而这里已经读取到落盘 Wasm 模块的真实导出列表。缺失导出
/// 会直接返回 `Err`，不会生成只在运行时才失败的启动壳。
fn validate_library_invoke_exports(wasm_path: &Path) -> Result<()> {
    let Some(spec) = read_library_invoke_spec(wasm_path)
    else {
        return Ok(());
    };
    let payload: serde_json::Value = serde_json::from_str(&spec).map_err(|error| format!("解析 nyar.library_invoke 自定义段失败: {error}"))?;
    let Some(glue_entries) = payload.get("glue_entries").and_then(|value| value.as_array())
    else {
        return Ok(());
    };
    let bytes = fs::read(wasm_path).map_err(|error| format!("读取 WASM 失败：{}: {error}", wasm_path.display()))?;
    let module = WasmBinaryModule::from_bytes(&bytes).map_err(|error| format!("WASM 解析失败：{error}"))?;
    let wasm_exports: BTreeSet<String> = parse_export_section(&module).into_iter().map(|item| item.name).collect();
    crate::lowering::backends::wasm::mir::library_glue::validate_glue_entry_exports(glue_entries, &wasm_exports)
}

fn library_invoke_marshaler_js(spec_json: &str) -> String {
    // LIBRARY_INVOKE 的 `returns` 与 marshal 目标消费结构化的 WasmAbiType JSON schema：
    // 要么是 string (旧格式 "i64")，要么是 object (新格式 {"kind": "I64", ...})。
    // `exports[*].params` 自 v2 起是参数身份对象 {index, name, type, carrier}，
    // 通过 `paramType` 取其中的 `type` 后再 marshal。
    format!(
        r#"const LIBRARY_INVOKE = {spec_json};

// ── Schema helpers ────────────────────────────────────────────────────────
// 从 param/return schema 提取 ABI kind 字符串。
// 同时兼容旧格式（直接是 "i64"）和新结构化格式（{{"kind": "I64"}}）。
function abiKind(schema) {{
    if (typeof schema === "string") return schema;
    if (schema && typeof schema.kind === "string") return schema.kind;
    return null;
}}

// 从 schema 提取 generic_args 的 kind 字符串数组（如 ArrayList<I64> → ["I64"]）。
function genericKinds(schema) {{
    if (!schema || !Array.isArray(schema.generic_args)) return [];
    return schema.generic_args.map(a => abiKind(a)).filter(Boolean);
}}

// 在 LIBRARY_INVOKE.glue_entries 里按 kind + module_path + name + genericKinds 查找匹配 entry。
// 匹配 key = `${{kind}}:${{modulePath}}:${{name}}:${{genericKinds.join(',')}}`。
// module_path 可能是空字符串（synthetic primitive helper），用 `e.module_path || ''` 安全访问。
function findGlueEntry(kind, modulePath, name, genericKindsList) {{
    const entries = LIBRARY_INVOKE?.glue_entries ?? [];
    for (const e of entries) {{
        if (e.kind !== kind) continue;
        if ((e.module_path || '') !== (modulePath || '')) continue;
        if (name !== undefined && e.name !== name) continue;
        if (genericKindsList && genericKindsList.length > 0) {{
            const ek = (e.generic_args ?? []).map(a => abiKind(a));
            if (ek.join(',') !== genericKindsList.join(',')) continue;
        }}
        return e;
    }}
    return null;
}}

// 判断 schema 是否自带 Wasm handle 载体（容器 / 具名聚合 / 可空）。
// 与后端 `abi_carrier_name` 的容器集合一致：这些类型在 Wasm 里是 GC 引用，
// JS 侧只能传递、不能构造；标量需要装箱，不存在 box/unbox 合同。
function hasWasmHandleCarrier(schema) {{
    const kind = abiKind(schema);
    return kind === "NamedAggregate" || kind === "Array" || kind === "FixedArray" || kind === "Tuple" || kind === "Option";
}}

// 检查 glue entry 的每个 operation 都有非空 export_name。
// Stage A 产出的 export_name 当前是空字符串，只有 Stage C 填了 valkyrie.v 侧真实导出名才算可用。
function entryHasExportNames(entry) {{
    if (!entry || !Array.isArray(entry.operations)) return false;
    for (const op of entry.operations) {{
        if (typeof op.export_name !== 'string' || op.export_name.length === 0) return false;
    }}
    return true;
}}

// 从 entry.operations 里按 operation 名查找 export_name。
function operationExportName(entry, operation) {{
    const op = (entry?.operations ?? []).find(o => o.operation === operation);
    return op ? op.export_name : null;
}}

// 从 param 条目提取 marshal 用的类型 schema。
// v2 合同是 {{index, name, type, carrier}}；兼容旧的裸 schema / 字符串条目。
function paramType(param) {{
    if (param && typeof param === "object" && param.type !== undefined) return param.type;
    return param;
}}

function marshalLibraryArg(schema, value) {{
    const kind = abiKind(schema);
    if (kind == null) return value;

    // ── Primitive carriers ──────────────────────────────────────────────
    if (kind === "I64") return BigInt(value ?? 0);
    if (kind === "I32" || kind === "F32" || kind === "F64") return Number(value ?? 0);
    if (kind === "Bool") return value ? 1 : 0;
    if (kind === "Char") return Number(value ?? 0);
    if (kind === "Utf8" || kind === "Utf16") {{
        // Utf8/Utf16 在 JS glue 轨是 i32 host-intern 句柄：JS 字符串入参在此登记为句柄，
        // 已是句柄的数值原样透传，避免把字符串当成裸 i32。
        return typeof value === "string" ? hostIntern(value) : Number(value ?? 0);
    }}
    if (kind === "Unit") return undefined;

    // ── NamedAggregate：从 glue_entries registry 查找 marshal 规则 ────
    // 不硬编码名称，完全消费 Stage A 生成的 glue_entries JSON 数组。
    if (kind === "NamedAggregate") {{
        const gk = genericKinds(schema);
        const modPath = schema.module_path || '';
        const entry = findGlueEntry("NamedAggregate", modPath, schema.name, gk);
        if (!entry) {{
            throw new Error(
                `[wasm-js-glue] Cannot marshal NamedAggregate ${{modPath}}.${{schema.name}}<${{gk.join(',')}}>: glue entry not found in LIBRARY_INVOKE.glue_entries. Compiler must emit glue_entries with this schema identity.`
            );
        }}
        if (!entryHasExportNames(entry)) {{
            const missingOps = (entry.operations || []).filter(o => !o.export_name).map(o => o.operation).join(',') || '(no operations with export_name)';
            throw new Error(
                `[wasm-js-glue] Cannot marshal NamedAggregate ${{modPath}}.${{schema.name}}<${{gk.join(',')}}>: glue entry found but missing export_name on operations: ${{missingOps}}. Compiler (Stage A/B/C) must fill export_name for all operations before JS glue can call them.`
            );
        }}
        const constructName = operationExportName(entry, "construct");
        const pushName = operationExportName(entry, "push");
        const construct = constructName ? exports[constructName] : null;
        const push = pushName ? exports[pushName] : null;
        if (!construct || !push) {{
            throw new Error(
                `[wasm-js-glue] Cannot marshal NamedAggregate ${{modPath}}.${{schema.name}}<${{gk.join(',')}}>: glue entry missing construct/push export. construct=${{constructName}}, push=${{pushName}}.`
            );
        }}
        // 元素 carrier 从 construct 或 push 的 param_carriers 推断（通常 push 的第二个参数是元素）
        const pushOp = entry.operations.find(o => o.operation === "push");
        const elementCarrier = (pushOp?.param_carriers ?? []).at(-1) ?? "i32";
        const arr = Array.isArray(value) ? value : [];
        const handle = construct(arr.length);
        for (const item of arr) {{
            let wasmItem = item;
            if (elementCarrier === "i64") wasmItem = BigInt(item ?? 0);
            else if (elementCarrier === "i32") wasmItem = Number(item ?? 0) | 0;
            else if (elementCarrier === "f64") wasmItem = Number(item ?? 0);
            else if (elementCarrier === "f32") wasmItem = Number(item ?? 0);
            push(handle, wasmItem);
        }}
        return handle;
    }}

    // ── 复合类型必须有 glue 合同，禁止 passthrough ──────────────────
    if (kind === "Array") {{
        const gk = genericKinds(schema);
        throw new Error(
            `[wasm-js-glue] Cannot marshal Array<${{gk.join(',')}}>: Array requires glue_entries contract. Please add a NamedAggregate-based glue entry.`
        );
    }}
    if (kind === "Option") {{
        // Option 是结构化容器：None 就是 null 引用，Some 就是内层值本身，不另生 glue entry。
        // 内层必须有 handle 载体（容器 / 具名聚合）；标量内层在 Wasm GC 里是装箱值，
        // 需要 box/unbox 的 glue 合同，当前未定义，失败关闭而不是把裸标量当引用传。
        if (!hasWasmHandleCarrier(schema.inner)) {{
            throw new Error(
                `[wasm-js-glue] Cannot marshal Option<${{abiKind(schema.inner)}}>: inner type has no Wasm handle carrier. Scalar payloads require a box/unbox glue contract that is not defined.`
            );
        }}
        if (value == null) return null;
        return marshalLibraryArg(schema.inner, value);
    }}
    if (kind === "Tuple") {{
        throw new Error(
            `[wasm-js-glue] Cannot marshal Tuple: heterogeneous tuples require glue_entries contract with per-element carriers. Tuple ABI bridge not implemented yet.`
        );
    }}

    // ── 未知 schema kind ────────────────────────────────────────────────
    throw new Error(
        `[wasm-js-glue] Cannot marshal schema kind "${{kind}}": no ABI bridge implemented. Schema: ${{JSON.stringify(schema)}}`
    );
}}

function demarshalLibraryReturn(schema, raw) {{
    const kind = abiKind(schema);
    if (kind == null) return raw;

    // ── Unit：无返回值 ──────────────────────────────────────────────────
    if (kind === "Unit") return undefined;

    // ── Primitive carriers ──────────────────────────────────────────────
    if (kind === "I64") return raw == null ? null : Number(raw);
    if (kind === "I32" || kind === "F32" || kind === "F64") return Number(raw);
    if (kind === "Bool") return raw !== 0 && raw !== null;
    if (kind === "Char") return Number(raw);
    if (kind === "Utf8" || kind === "Utf16") return raw == null ? "" : hostResolve(raw);

    // ── NamedAggregate：从 glue_entries registry 查找 demarshal 规则 ────
    if (kind === "NamedAggregate") {{
        if (raw == null) return null;
        const gk = genericKinds(schema);
        const modPath = schema.module_path || '';
        const entry = findGlueEntry("NamedAggregate", modPath, schema.name, gk);
        if (!entry) {{
            throw new Error(
                `[wasm-js-glue] Cannot demarshal NamedAggregate ${{modPath}}.${{schema.name}}<${{gk.join(',')}}>: glue entry not found in LIBRARY_INVOKE.glue_entries. Compiler must emit glue_entries with this schema identity.`
            );
        }}
        if (!entryHasExportNames(entry)) {{
            const missingOps = (entry.operations || []).filter(o => !o.export_name).map(o => o.operation).join(',') || '(no operations with export_name)';
            throw new Error(
                `[wasm-js-glue] Cannot demarshal NamedAggregate ${{modPath}}.${{schema.name}}<${{gk.join(',')}}>: glue entry found but missing export_name on operations: ${{missingOps}}. Compiler (Stage A/B/C) must fill export_name for all operations before JS glue can call them.`
            );
        }}
        const lengthName = operationExportName(entry, "length");
        const atName = operationExportName(entry, "at");
        const lengthFn = lengthName ? exports[lengthName] : null;
        const atFn = atName ? exports[atName] : null;
        if (!lengthFn || !atFn) {{
            throw new Error(
                `[wasm-js-glue] Cannot demarshal NamedAggregate ${{modPath}}.${{schema.name}}<${{gk.join(',')}}>: glue entry missing length/at export. length=${{lengthName}}, at=${{atName}}.`
            );
        }}
        const len = Number(lengthFn(raw));
        if (len === 0) return [];
        const out = [];
        // 返回值 carrier 推断自 at 操作的 return_carrier
        const atOp = entry.operations.find(o => o.operation === "at");
        const returnCarrier = atOp?.return_carrier ?? "i32";
        for (let i = 0; i < len; i++) {{
            // V `ArrayList.at` 使用序数索引（1-based）；JS 循环 `i` 是基数（0-based），故传 `i + 1`。
            let v = atFn(raw, i + 1);
            if (returnCarrier === "i64") v = Number(v);
            else v = Number(v);
            out.push(v);
        }}
        return out;
    }}

    // ── 复合类型必须有 glue 合同，禁止 passthrough ──────────────────
    if (kind === "Array") {{
        const gk = genericKinds(schema);
        throw new Error(
            `[wasm-js-glue] Cannot demarshal Array<${{gk.join(',')}}>: Array requires glue_entries contract. Please add a NamedAggregate-based glue entry.`
        );
    }}
    if (kind === "Option") {{
        // 与 marshal 对称：null 引用即 None；Some 的内层按自身 schema 反序列化。
        if (!hasWasmHandleCarrier(schema.inner)) {{
            throw new Error(
                `[wasm-js-glue] Cannot demarshal Option<${{abiKind(schema.inner)}}>: inner type has no Wasm handle carrier. Scalar payloads require a box/unbox glue contract that is not defined.`
            );
        }}
        if (raw == null) return null;
        return demarshalLibraryReturn(schema.inner, raw);
    }}
    if (kind === "Tuple") {{
        throw new Error(
            `[wasm-js-glue] Cannot demarshal Tuple: heterogeneous tuples require glue_entries contract with per-element carriers. Tuple ABI bridge not implemented yet.`
        );
    }}

    // ── 未知 schema kind ────────────────────────────────────────────────
    throw new Error(
        `[wasm-js-glue] Cannot demarshal schema kind "${{kind}}": no ABI bridge implemented. Schema: ${{JSON.stringify(schema)}}`
    );
}}
"#,
        spec_json = spec_json
    )
}

/// 构建 `Node` 启动壳（`.mjs`）。
///
/// 启动壳只做三件事：读取 `WASM` 字节码、按声明的导入构造 `importObject`、
/// 调用 `exports.main ?? exports._start`。不输出 banner、不解析 CLI 子命令、
/// 不注入伪导入，确保非 CLI 产物的 `stdout` 不被污染。
#[doc(hidden)]
pub fn build_node_launcher(
    artifact_name: &str,
    imports: &[(String, String)],
    utf8_literals: &[String],
    library_mode: bool,
    library_invoke: Option<&str>,
) -> String {
    let utf8_literals_json = serde_json::to_string(utf8_literals).unwrap_or_else(|_| "[]".to_string());
    let has_imports = !imports.is_empty();
    let has_read_source = imports.iter().any(|(_, field)| field == "read_source_byte");
    let smoke_import_object_literal = render_import_object_literal(imports);
    // ── CLI 模式开关：仅基于 import 名存在性 ────────────────────────────
    // 注意：这里只看 `"cli_get_target"` / `"cli_get_project"` / `"cli_get_output"`
    // 这三个 import **名**是否同时存在，用来决定启动壳是否生成 CLI 分派代码。
    //
    // ❌ 这里**绝不**读取编译目标（`TargetExecutionPlan.target`）或 runner
    //    默认值 `"node"` —— `cli_target` 的值是 Node 启动壳内部 JS 变量，
    //    和编译目标事实是两条完全独立的语义线。
    let cli_mode = imports.iter().any(|(_, field)| field == "cli_get_project")
        && imports.iter().any(|(_, field)| field == "cli_get_target")
        && imports.iter().any(|(_, field)| field == "cli_get_output");

    let smoke_import_object = if library_mode {
        if has_imports {
            format!(
                r#"const importObject = {import_object};
let wasmInstance;
let exports;"#,
                import_object = smoke_import_object_literal
            )
        }
        else {
            r#"const importObject = {};
let wasmInstance;
let exports;"#
                .to_string()
        }
    }
    else if has_imports {
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
// ── Node runner CLI 默认值，不是编译目标 ────────────────────────────
// 本变量仅用于 Node 启动壳内部的 `--target` CLI 参数默认值。
// WASM 模块内部通过 `cli_get_target` import 读取到的就是这个运行时值，
// 供测试矩阵选择执行宿主（node / deno / bun ...）。
//
// ⚠️  它和 `TargetExecutionPlan.target` / planner manifest 里的**编译目标事实**
//      完全无关。emitter 构建链路在 Rust 侧从不读取本变量的值——
//      `cli_mode` 开关（决定是否生成 CLI 分派代码）只看 `"cli_get_target"`
//      这个 import **名**是否存在，与默认值字符串无关。
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

{arg_parsing}const LIBRARY_MODE = {library_mode};
const wasmBytes = readFileSync(new URL("./{name}.wasm", import.meta.url));
{import_object}
{library_exports}
{cli_dispatch}
{executable_tail}"#,
        name = artifact_name,
        arg_parsing = smoke_arg_parsing,
        import_object = smoke_import_object,
        library_exports = if library_mode {
            let marshaler = library_invoke.map(library_invoke_marshaler_js).unwrap_or_default();
            format!(
                r#"{marshaler}export async function callExport(name, ...args) {{
    if (!exports) {{
        wasmInstance = await wasmResolveInstance(wasmBytes, importObject);
        exports = wasmInstance.exports;
    }}
    const fn = exports[name];
    if (typeof fn !== "function") {{
        throw new Error(`wasm export not found: ${{name}}`);
    }}
    const spec = LIBRARY_INVOKE?.exports?.[name];
    if (spec?.params) {{
        const marshaled = spec.params.map((param, index) => marshalLibraryArg(paramType(param), args[index]));
        const raw = fn(...marshaled);
        return demarshalLibraryReturn(spec.returns, raw);
    }}
    return fn(...args);
}}
"#,
                marshaler = marshaler
            )
        }
        else {
            String::new()
        },
        cli_dispatch = if library_mode { "" } else { cli_dispatch },
        executable_tail = if library_mode {
            String::new()
        }
        else {
            format!(
                r#"const exports = instance.exports;
const entry = exports.main ?? exports._start;
let result;
if (typeof entry === "function") {{
    result = entry();
}}
{output_logic}"#,
                output_logic = smoke_output_logic
            )
        },
        utf8_literals_json = utf8_literals_json,
        library_mode = if library_mode { "true" } else { "false" },
    )
}

/// 从 `WASM` 自定义段读取 `UTF-8` 字面量表。
///
/// 读取名为 `nyar.strings` 的自定义段（由 `lowering` 后端写入），
/// 使用发射器命名空间而非产品名，作为跨后端的线格式契约。
fn read_wasm_utf8_literals(wasm_path: &Path) -> Result<Vec<String>> {
    let bytes = fs::read(wasm_path).map_err(|e| format!("读取 WASM 失败：{}: {e}", wasm_path.display()))?;
    let module = WasmBinaryModule::from_bytes(&bytes).map_err(|error| format!("WASM 解析失败：{error}"))?;
    for section in &module.sections {
        if section.name.as_deref() == Some("nyar.strings") {
            let payload: serde_json::Value =
                serde_json::from_slice(&section.bytes).map_err(|e| format!("解析 nyar.strings 自定义段失败: {e}"))?;
            if let Some(values) = payload.get("strings").and_then(|value| value.as_array()) {
                return Ok(values.iter().filter_map(|value| value.as_str().map(str::to_string)).collect());
            }
        }
    }
    Ok(Vec::new())
}
