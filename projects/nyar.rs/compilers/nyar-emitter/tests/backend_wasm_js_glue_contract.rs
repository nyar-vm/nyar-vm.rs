//! Node JS glue 合同层覆盖：`build_node_launcher`（library 模式）生成的 `.mjs`
//! 是交付产物本体，本文件按 marshal/demarshal 各形态锁定其真实合同——
//! 标量、Utf8 句柄、序列族 length/at、handle 载体规则、`pending_export_name`
//! 失败闭合与导入表渲染。
//!
//! 与 `backend_wasm_js_glue.rs`（launcher 骨架/CLI 隔离）互补：本文件只覆盖
//! library 模式的 ABI 桥合同。端到端行为（真实构建 + Node 调用）在
//! `rusty-valkyrie/tests/asgard/glue_contract.rs`，两处不重复造求值。

use nyar_emitter::{build_node_launcher, collect_import_bindings};

/// 构造 library_invoke v0 规格：单个导出 + 可选 glue_entries。
fn spec(exports: &str, glue_entries: &str) -> String {
    format!(r#"{{"version":0,"exports":{exports},"glue_entries":{glue_entries},"abi_digest":"sha256-fixture"}}"#)
}

const NO_ENTRIES: &str = "[]";

fn library_glue(spec: &str) -> String {
    build_node_launcher("contract_demo", &[], &[], true, Some(spec))
}

// ── Utf8：host-intern 句柄与 JS 字符串双向桥 ───────────────────────────────

#[test]
fn glue_demarshals_utf8_return_through_host_intern_table() {
    // `Utf8` 返回在 JS glue 轨是宿主 host-intern 句柄；demarshal 必须经
    // `hostResolve` 解码回 JS 字符串，而不是把句柄当数字交给调用方。
    let glue = library_glue(&spec(r#"{"title":{"params":[],"returns":{"kind":"Utf8"}}}"#, NO_ENTRIES));
    assert!(
        glue.contains(r#"if (kind === "Utf8" || kind === "Utf16") return raw == null ? "" : hostResolve(raw);"#),
        "Utf8 返回必须经 host-intern 表解码为字符串:\n{glue}"
    );
}

#[test]
fn glue_marshals_js_string_utf8_parameter_to_host_handle() {
    // Utf8/Utf16 参数位接受 JS 字符串并在宿主表登记句柄；数值句柄原样透传，
    // 不把字符串当裸 i32。
    let glue = library_glue(&spec(
        r#"{"echo":{"params":[{"index":0,"name":"s","type":{"kind":"Utf8"},"carrier":"i32"}],"returns":{"kind":"Utf8"}}}"#,
        NO_ENTRIES,
    ));
    assert!(
        glue.contains(r#"return typeof value === "string" ? hostIntern(value) : Number(value ?? 0);"#),
        "JS 字符串入参必须登记为 host-intern 句柄:\n{glue}"
    );
}

// ── 序列族聚合：length/at demarshal、construct/push marshal ────────────────

/// `ArrayList<I64>` 的 ready glue entry（export 名由 plan 声明）。
const READY_LIST_ENTRY: &str = r#"[{
    "kind":"NamedAggregate","name":"ArrayList","module_path":"std.collections","generic_args":[{"kind":"I64"}],
    "operations":[
        {"operation":"construct","export_name":"$seq_list_new","param_carriers":["i32"],"return_carrier":"handle","export_source":"declared"},
        {"operation":"push","export_name":"$seq_list_push","param_carriers":["handle","i64"],"return_carrier":"void","export_source":"declared"},
        {"operation":"length","export_name":"$seq_list_len","param_carriers":["handle"],"return_carrier":"i32","export_source":"declared"},
        {"operation":"at","export_name":"$seq_list_at","param_carriers":["handle","i32"],"return_carrier":"i64","export_source":"declared"}
    ],
    "source":"plan","status":"ready"
}]"#;

#[test]
fn glue_demarshals_sequence_aggregate_via_length_and_at_one_based() {
    // 序列族 demarshal 合同：`length(handle)` 取长度、`at(handle, i + 1)` 逐项读回。
    // `+1` 是 V 侧 1 基序数与 JS 0 基下标的唯一换算点，不得丢失。
    let glue = library_glue(&spec(
        r#"{"sweep":{"params":[],"returns":{"kind":"NamedAggregate","name":"ArrayList","module_path":"std.collections","generic_args":[{"kind":"I64"}]}}}"#,
        READY_LIST_ENTRY,
    ));
    assert!(glue.contains("const len = Number(lengthFn(raw));"), "长度必须来自 length operation export:\n{glue}");
    assert!(glue.contains("atFn(raw, i + 1)"), "demarshal 必须按 1 基序数调用 at:\n{glue}");
    assert!(glue.contains("if (len === 0) return [];"), "空序列必须读回空数组而不是 null:\n{glue}");
    assert!(glue.contains("\"$seq_list_len\"") && glue.contains("\"$seq_list_at\""), "glue 必须逐字携带 plan 声明的 export 名:\n{glue}");
}

#[test]
fn glue_marshal_constructs_sequence_aggregate_via_construct_and_push() {
    // 序列族 marshal 合同：`construct(n)` 铸造句柄后逐项 `push(handle, item)`，
    // 元素按 push 末位 param_carrier 归一（i64 → BigInt）。
    let glue = library_glue(&spec(
        r#"{"feed":{"params":[{"index":0,"name":"items","type":{"kind":"NamedAggregate","name":"ArrayList","module_path":"std.collections","generic_args":[{"kind":"I64"}]},"carrier":"handle"}],"returns":{"kind":"Unit"}}}"#,
        READY_LIST_ENTRY,
    ));
    assert!(glue.contains("const handle = construct(arr.length);"), "marshal 必须经 construct 铸造句柄:\n{glue}");
    assert!(glue.contains("push(handle, wasmItem);"), "marshal 必须逐项 push:\n{glue}");
    assert!(glue.contains(r#"if (elementCarrier === "i64") wasmItem = BigInt(item ?? 0);"#), "i64 元素必须 BigInt 化以匹配 Wasm 槽位:\n{glue}");
}

// ── pending_export_name：非序列/未填名聚合失败闭合 ─────────────────────────

/// 非序列聚合（无 `[T]` 结构事实）的 entry：operations 为空、状态停在
/// `pending_export_name`，JS 侧必须拒绝 marshal/demarshal 而不是猜字段访问。
const PENDING_ENTRY: &str = r#"[{
    "kind":"NamedAggregate","name":"ConfigArtifact","module_path":"","generic_args":[],
    "operations":[],"source":"declaration_identity","status":"pending_export_name"
}]"#;

#[test]
fn glue_refuses_aggregate_without_ready_operations_contract() {
    let glue = library_glue(&spec(
        r#"{"configure":{"params":[],"returns":{"kind":"NamedAggregate","name":"ConfigArtifact","module_path":"","generic_args":[]}}}"#,
        PENDING_ENTRY,
    ));
    // 聚合身份逐字携带（供运行时按 identity 查表）。
    assert!(glue.contains("\"status\":\"pending_export_name\""), "编译器未填 export_name 时状态必须原样进入产物:\n{glue}");
    // 运行时守卫：无可用 operations 时按「缺 export_name」失败闭合。
    assert!(glue.contains("missing export_name on operations"), "pending 聚合必须显式抛错，不得静默返回句柄:\n{glue}");
    assert!(glue.contains("glue entry not found in LIBRARY_INVOKE.glue_entries"), "查无 entry 的聚合必须指明编译器责任:\n{glue}");
    // 转红触发器：字段访问合同（非序列聚合 demarshal）落地后，本用例升级为
    // 「按字段读回对象」的端到端断言（归后端 glue/ABI 与配置 SDK 所有者）。
}

// ── handle 载体规则：无桥复合类型一律失败闭合 ─────────────────────────────

#[test]
fn glue_fails_closed_for_unbridged_composite_kinds() {
    let glue = library_glue(&spec(r#"{"noop":{"params":[],"returns":{"kind":"Unit"}}}"#, NO_ENTRIES));
    // Array / Tuple 没有独立 ABI 桥：必须要求 glue_entries 合同而不是 passthrough。
    assert!(glue.contains("Cannot marshal Array"), "Array marshal 必须失败闭合:\n{glue}");
    assert!(glue.contains("Cannot demarshal Array"), "Array demarshal 必须失败闭合:\n{glue}");
    assert!(glue.contains("Cannot marshal Tuple"), "Tuple marshal 必须失败闭合:\n{glue}");
    assert!(glue.contains("Cannot demarshal Tuple"), "Tuple demarshal 必须失败闭合:\n{glue}");
    // Option 内层没有 handle 载体时（标量载荷需要 box/unbox 合同）拒绝裸标量。
    assert!(glue.contains("inner type has no Wasm handle carrier"), "Option 标量内层必须失败闭合:\n{glue}");
    // 未知 schema kind 不静默 passthrough。
    assert!(glue.contains("no ABI bridge implemented"), "未知 kind 必须失败闭合:\n{glue}");
}

#[test]
fn glue_handle_carrier_set_matches_backend_abi() {
    // JS 侧判定「自带 Wasm handle 载体」的集合必须与后端 abi_carrier_name 的
    // handle 分支一致：NamedAggregate / Array / FixedArray / Tuple / Option。
    let glue = library_glue(&spec(r#"{"noop":{"params":[],"returns":{"kind":"Unit"}}}"#, NO_ENTRIES));
    for kind in ["NamedAggregate", "Array", "FixedArray", "Tuple", "Option"] {
        assert!(glue.contains(&format!(r#"kind === "{kind}""#)), "handle 载体集合必须包含 {kind}:\n{glue}");
    }
}

// ── 调用面：callExport 查找与规格驱动 marshal ─────────────────────────────

#[test]
fn glue_call_export_resolves_spec_params_by_position_and_identity() {
    // callExport 合同：按 LIBRARY_INVOKE.exports[name].params 逐位 marshal，
    // 返回按 returns schema demarshal；无规格命中时原样透传（宿主 helper 路径）。
    let glue = library_glue(&spec(
        r#"{"solve":{"params":[{"index":0,"name":"n","type":{"kind":"I64"},"carrier":"i64"}],"returns":{"kind":"I64"}}}"#,
        NO_ENTRIES,
    ));
    assert!(glue.contains("export async function callExport"), "library glue 必须暴露 callExport:\n{glue}");
    assert!(glue.contains(r#"const spec = LIBRARY_INVOKE?.exports?.[name];"#), "调用规格必须按导出名查表:\n{glue}");
    assert!(
        glue.contains("spec.params.map((param, index) => marshalLibraryArg(paramType(param), args[index]))"),
        "参数必须按声明顺序逐位 marshal:\n{glue}"
    );
    assert!(glue.contains("demarshalLibraryReturn(spec.returns, raw)"), "返回值必须按返回 schema demarshal:\n{glue}");
    assert!(glue.contains(r#"if (kind === "I64") return BigInt(value ?? 0);"#), "i64 参数必须 BigInt 化:\n{glue}");
}

#[test]
fn glue_library_mode_never_auto_runs_or_exits() {
    // library 产物是被动调用面：不得自动执行 main/_start，不得以退出码结束宿主。
    let glue = library_glue(&spec(r#"{"noop":{"params":[],"returns":{"kind":"Unit"}}}"#, NO_ENTRIES));
    assert!(!glue.contains("exports.main ?? exports._start"), "library glue 不得自动运行入口:\n{glue}");
    assert!(!glue.contains("process.exit"), "library glue 不得替宿主决定退出码:\n{glue}");
    assert!(glue.contains("let wasmInstance;"), "实例必须惰性构造（首次 callExport 才实例化）:\n{glue}");
}

// ── 导入表：只渲染声明的导入，按模块分组，非标识符字段加引号 ───────────────

#[test]
fn glue_import_object_groups_by_module_and_quotes_non_identifier_fields() {
    let imports = vec![
        ("env".to_string(), "add".to_string()),
        ("env".to_string(), "add".to_string()),
        ("std.text".to_string(), "utf8 concat".to_string()),
    ];
    let bindings = collect_import_bindings(&imports);
    assert_eq!(bindings.len(), 2, "重复导入必须去重");

    let glue = build_node_launcher("demo", &imports, &[], false, None);
    assert!(glue.contains("env: {"), "导入表必须按模块分组:\n{glue}");
    assert!(glue.contains(r#""std.text": {"#), "非标识符模块名必须加引号:\n{glue}");
    assert!(glue.contains(r#""utf8 concat":"#) || glue.contains(r#""utf8 concat": "#), "非标识符字段名必须加引号:\n{glue}");
    assert!(glue.contains("add: (a, b) => (a + b) | 0"), "已声明字段必须有宿主实现:\n{glue}");
}

#[test]
fn glue_unknown_import_field_falls_back_to_zero_impl() {
    // 现状锁定：未认识的导入字段生成 `() => 0` 占位实现（导入表按字段名查实现，
    // 未知字段不阻断产物生成）。这是当前真实合同；若未来改为编译期拒绝未映射
    // 导入，本用例转红并升级为拒绝断言。
    let imports = vec![("env".to_string(), "mystery_import".to_string())];
    let glue = build_node_launcher("demo", &imports, &[], false, None);
    assert!(glue.contains("mystery_import: () => 0"), "未知导入字段当前必须落 () => 0:\n{glue}");
}
