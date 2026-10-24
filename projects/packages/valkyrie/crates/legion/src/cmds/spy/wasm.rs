#![doc = include_str!("readme.md")]

use std::{fs, path::Path, process::ExitCode};

use acorn_wasm::{
    DecodedInstruction, DecodedOperand, SECTION_CUSTOM, WasmBinaryModule, WasmExternalKind, WasmFunctionEntry, WasmImport, WasmTypeEntry,
    WasmTypeKind, decode_code_body, parse_code_section, parse_export_section, parse_import_section, parse_type_section, section_name,
    uleb128_size, wasm_value_type_name,
};
use miette::{IntoDiagnostic, Result, miette};

use super::{SpyOptions, SpyTargetOptions};

/// 执行 WASM 二进制反汇编。
///
/// 用法：`legion spy wasm <file> [--func <index|name>] [--list] [--offset <abs>] [--json] [--hex]`
pub fn run(options: &SpyOptions) -> Result<ExitCode> {
    let (_, opts) = options.split();
    let Some(target) = &opts.input
    else {
        return Err(miette!(
            r#"用法：legion spy wasm <file> [--func <index|name>] [--list] [--offset <abs>] [--json] [--hex] [--glue-audit]
  file             目标 WASM / WASI component（`.wasm` / `.wasi` / `.core.wasm`）
  --func <i|name>  反汇编指定函数（索引或名称；仅 core module）
  --list           列出所有函数 / 段（component 则列出 section id）
  --offset <abs>   定位绝对偏移量处的指令（仅 core module）
  --json           以 JSON 格式输出
  --hex            dump 函数体原始字节（配合 --func）
  --glue-audit     审计 Node JS-glue：cli_get_* 导入与 help/version/build/main 导出匹配性"#
        ));
    };

    if !Path::exists(Path::new(target)) {
        return Err(miette!("文件不存在：{}", target));
    }

    let data = fs::read(target).into_diagnostic().map_err(|error| error.wrap_err(format!("无法读取文件 {}", target)))?;

    // Component-model binaries share the `\0asm` magic but use a non-1 version.
    // Prefer a dedicated overview over failing the core-module parser.
    if let Some(component_version) = detect_component_version(&data) {
        return dump_component_overview(target, &data, component_version, opts);
    }

    let module = WasmBinaryModule::from_bytes(&data).map_err(|error| miette!("WASM 解析失败：{error}"))?;

    // Type 段结构化解析模式（优先级仅次于偏移定位）
    if opts.types {
        return dump_type_section(&module, &data, opts);
    }

    // GC layout 覆盖审计
    if opts.gc_audit {
        return dump_gc_audit(&module, &data, opts);
    }

    // Node JS-glue 契约审计
    if opts.glue_audit {
        return dump_glue_audit(&module, opts);
    }

    // 偏移定位模式优先级最高
    if let Some(offset) = opts.offset {
        return locate_offset(&data, offset as usize, opts);
    }

    // 函数反汇编模式
    if let Some(func_spec) = &opts.func {
        return disassemble_function(&module, &data, func_spec, opts);
    }

    // 默认 / 列表模式
    print_overview(&module, &data, opts);
    Ok(ExitCode::SUCCESS)
}

/// 打印模块概览。
fn print_overview(module: &WasmBinaryModule, data: &[u8], opts: &SpyTargetOptions) {
    if opts.json {
        print_json_overview(module, data);
        return;
    }

    println!("WASM 模块（{} 字节，版本 {}）", data.len(), module.version);
    println!("段数：{}", module.sections.len());
    println!();

    // 段摘要
    println!("=== 段列表 ===");
    for (index, section) in module.sections.iter().enumerate() {
        let name = section_name(section.id);
        let detail =
            if section.id == SECTION_CUSTOM { section.name.clone().unwrap_or_default() } else { format!("{} 字节", section.bytes.len()) };
        println!("  [{}] id={} {:<16} {}", index, section.id, name, detail);
    }
    println!();

    // imports
    let imports = parse_import_section(module);
    if !imports.is_empty() {
        println!("=== Imports（{} 项）===", imports.len());
        for imp in &imports {
            println!("  {:<12} {}.{} : {}", imp.kind.name(), imp.module, imp.field, format_import_type(imp));
        }
        println!();
    }

    // exports
    let exports = parse_export_section(module);
    if !exports.is_empty() {
        println!("=== Exports（{} 项）===", exports.len());
        for exp in &exports {
            println!("  {:<12} {} : index={}", exp.kind.name(), exp.name, exp.index);
        }
        println!();
    }

    // 函数列表
    let functions = parse_code_section(module);
    if !functions.is_empty() {
        println!("=== 函数（{} 项）===", functions.len());
        for func in &functions {
            let name_hint = func.name.as_deref().unwrap_or("<unnamed>");
            println!("  [{}] {} ({} 字节, {} 局部变量组)", func.index, name_hint, func.body_len, func.local_groups);
        }
    }
    else {
        println!("（无 Code 段，模块不含函数体）");
    }
}

/// 反汇编指定函数。
fn disassemble_function(module: &WasmBinaryModule, data: &[u8], func_spec: &str, opts: &SpyTargetOptions) -> Result<ExitCode> {
    let functions = parse_code_section(module);
    let mir_functions = parse_nyar_wasm_functions(module);

    // 按索引、导出名，或 `legion.wasm.functions` 中的完整 MIR 符号精确查找。
    let target_func = if let Ok(index) = func_spec.parse::<usize>() {
        functions.iter().find(|f| f.index == index)
    }
    else {
        functions.iter().find(|f| f.name.as_deref() == Some(func_spec)).or_else(|| {
            mir_functions
                .iter()
                .find_map(|(index, symbol)| (symbol == func_spec).then(|| functions.iter().find(|f| f.index == *index)).flatten())
        })
    };

    let Some(func) = target_func
    else {
        return Err(miette!(
            "未找到函数 '{}'，当前共有 {} 个函数（索引 0..{}）；可用导出名、函数索引或 MIR 符号（如 `legion::execute_build_from_cli`）",
            func_spec,
            functions.len(),
            functions.len().saturating_sub(1)
        ));
    };

    if opts.hex {
        // 原始字节 dump
        let body = read_function_body(data, func);
        print_hex_dump(&body, func);
        return Ok(ExitCode::SUCCESS);
    }

    let instructions = disassemble_function_body(data, func);

    if opts.json {
        print_json_function(func, &instructions);
        return Ok(ExitCode::SUCCESS);
    }

    // 反汇编
    println!("=== 函数 {} ===", func.index);
    if let Some(name) = &func.name {
        println!("名称: {}", name);
    }
    if let Some(mir_symbol) = mir_functions.get(&func.index) {
        println!("MIR: {}", mir_symbol);
    }
    println!("代码偏移: 0x{:04X}", func.code_offset);
    println!("代码长度: {} 字节", func.body_len);
    println!("局部变量组: {}", func.local_groups);
    println!();

    print_instructions(&instructions);
    Ok(ExitCode::SUCCESS)
}

/// 按绝对偏移定位指令上下文。
fn locate_offset(data: &[u8], offset: usize, opts: &SpyTargetOptions) -> Result<ExitCode> {
    if offset >= data.len() {
        return Err(miette!("偏移 {} 超出文件长度 {}", offset, data.len()));
    }

    let module = WasmBinaryModule::from_bytes(data).map_err(|error| miette!("WASM 解析失败：{error}"))?;

    let functions = parse_code_section(&module);

    // 查找包含该偏移的函数
    let containing = functions.iter().find(|f| {
        let start = f.code_offset;
        let end = f.code_offset + f.body_len;
        offset >= start && offset < end
    });

    if opts.json {
        print_json_offset(offset, containing, data);
        return Ok(ExitCode::SUCCESS);
    }

    if let Some(func) = containing {
        println!("偏移 0x{:04X} 位于函数 {} 中", offset, func.index);
        if let Some(name) = &func.name {
            println!("函数名: {}", name);
        }
        let mir_functions = parse_nyar_wasm_functions(&module);
        if let Some(mir_symbol) = mir_functions.get(&func.index) {
            println!("MIR: {}", mir_symbol);
        }
        println!("函数代码范围: 0x{:04X}..0x{:04X}", func.code_offset, func.code_offset + func.body_len);
        println!();

        // 反汇编该函数并高亮目标偏移
        let instructions = disassemble_function_body(data, func);
        let context = opts.context;
        let target_pos = instructions.iter().enumerate().find_map(|(i, instr)| {
            let end = instructions.get(i + 1).map(|n| n.offset).unwrap_or(instr.offset + 1);
            if instr.offset == offset || (instr.offset <= offset && offset < end) { Some(i) } else { None }
        });

        match target_pos {
            Some(center) => {
                let start = center.saturating_sub(context);
                let end = (center + context + 1).min(instructions.len());
                for (i, instr) in instructions[start..end].iter().enumerate() {
                    let marker = if i == center - start { ">>>" } else { "   " };
                    println!("{} {:>4}:  {:<20} {}", marker, instr.offset, instr.mnemonic, format_operands(&instr.operands));
                }
            }
            None => {
                println!("（未找到精确匹配的指令，附近字节：）");
                let start = offset.saturating_sub(16);
                let end = (offset + 16).min(data.len());
                print_hex_range(data, start, end, offset);
            }
        }
    }
    else {
        println!("偏移 0x{:04X} 不在任何函数体内", offset);
        // 查找该偏移属于哪个段
        let mut cursor = 8; // 跳过 magic + version
        for section in &module.sections {
            let section_start = cursor;
            let name_overhead = if section.id == SECTION_CUSTOM {
                section.name.as_ref().map(|n| n.len() + uleb128_size(n.len() as u32)).unwrap_or(0)
            }
            else {
                0
            };
            let payload_len = name_overhead + section.bytes.len();
            let section_end = cursor + 1 + uleb128_size(payload_len as u32) + payload_len;
            if offset >= section_start && offset < section_end {
                println!("位于段 id={} ({})", section.id, section_name(section.id));
                break;
            }
            cursor = section_end;
        }
        println!();
        let start = offset.saturating_sub(16);
        let end = (offset + 16).min(data.len());
        print_hex_range(data, start, end, offset);
    }

    Ok(ExitCode::SUCCESS)
}

// ========== 函数体辅助 ==========

/// 读取函数体原始字节。
fn read_function_body(data: &[u8], func: &WasmFunctionEntry) -> Vec<u8> {
    let start = func.code_offset;
    let end = (start + func.body_len).min(data.len());
    data[start..end].to_vec()
}

/// 反汇编函数体指令。
///
/// 委托 `acorn-wasm` 的 `decode_code_body` 完成局部变量跳过与逐条解码，
/// 再将相对偏移修正为文件绝对偏移，以保持 `spy` 的偏移语义。
fn disassemble_function_body(data: &[u8], func: &WasmFunctionEntry) -> Vec<DecodedInstruction> {
    let body_start = func.code_offset;
    let body_end = (body_start + func.body_len).min(data.len());

    let mut instructions = decode_code_body(&data[body_start..body_end]);
    for instr in &mut instructions {
        instr.offset += body_start;
    }
    instructions
}

// ========== legion custom section 解析 ==========

fn custom_section_bytes(module: &WasmBinaryModule, name: &str) -> Option<Vec<u8>> {
    module
        .sections
        .iter()
        .find(|section| section.id == SECTION_CUSTOM && section.name.as_deref() == Some(name))
        .map(|section| section.bytes.clone())
}

fn parse_nyar_wasm_functions(module: &WasmBinaryModule) -> std::collections::HashMap<usize, String> {
    let Some(bytes) = custom_section_bytes(module, "legion.wasm.functions")
    else {
        return std::collections::HashMap::new();
    };
    let text = String::from_utf8_lossy(&bytes);
    let mut map = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("import_count=") {
            continue;
        }
        let Some((index, symbol)) = line.split_once('\t')
        else {
            continue;
        };
        if let Ok(wasm_index) = index.parse::<usize>() {
            map.insert(wasm_index, symbol.to_string());
        }
    }
    map
}

/// 审计 Node / JS-glue 契约：CLI 导入是否与启动壳所需导出匹配。
///
/// `.mjs` 启动壳在同时存在 `cli_get_project` / `cli_get_target` / `cli_get_output` 时进入 CLI 模式，
/// 并要求 `help` / `version` / `build` 导出。本审计用于在 Node 运行前用 spy 发现不匹配。
fn dump_glue_audit(module: &WasmBinaryModule, opts: &SpyTargetOptions) -> Result<ExitCode> {
    let imports = parse_import_section(module);
    let exports = parse_export_section(module);
    let import_fields: Vec<&str> = imports.iter().filter(|imp| imp.module == "env").map(|imp| imp.field.as_str()).collect();
    let export_names: Vec<&str> = exports.iter().map(|exp| exp.name.as_str()).collect();

    let has_cli_project = import_fields.contains(&"cli_get_project");
    let has_cli_target = import_fields.contains(&"cli_get_target");
    let has_cli_output = import_fields.contains(&"cli_get_output");
    let cli_mode = has_cli_project && has_cli_target && has_cli_output;
    let has_main = export_names.iter().any(|name| *name == "main" || *name == "_start");
    let has_help = export_names.contains(&"help");
    let has_version = export_names.contains(&"version");
    let has_build = export_names.contains(&"build");

    let mut warnings = Vec::new();
    if cli_mode {
        if !has_help {
            warnings.push("CLI 模式导入齐全，但缺少 export `help`（`node <launcher> --help` / 空 argv 会失败）");
        }
        if !has_version {
            warnings.push("CLI 模式导入齐全，但缺少 export `version`（`node <launcher> --version` 会失败）");
        }
        if !has_build {
            warnings.push("CLI 模式导入齐全，但缺少 export `build`（`node <launcher> build ...` 会失败）");
        }
    }
    if !has_main {
        warnings.push("缺少 export `main` / `_start`（启动壳入口分派会失败）");
    }

    if opts.json {
        let payload = serde_json::json!({
            "cli_mode": cli_mode,
            "imports": import_fields,
            "exports": export_names,
            "has_main": has_main,
            "has_help": has_help,
            "has_version": has_version,
            "has_build": has_build,
            "warnings": warnings,
            "ok": warnings.is_empty(),
        });
        println!("{}", serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()));
        return Ok(if warnings.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE });
    }

    println!("=== Node JS-glue 契约审计 ===");
    println!("  CLI 模式（cli_get_project/target/output 齐全）: {}", if cli_mode { "是" } else { "否" });
    println!("  Imports (env): {}", if import_fields.is_empty() { "<none>".to_string() } else { import_fields.join(", ") });
    println!("  Exports: {}", if export_names.is_empty() { "<none>".to_string() } else { export_names.join(", ") });
    println!("  main/_start: {}  help: {}  version: {}  build: {}", has_main, has_help, has_version, has_build);
    if warnings.is_empty() {
        println!("  结果: OK");
        Ok(ExitCode::SUCCESS)
    }
    else {
        println!("  结果: 发现问题");
        for warning in &warnings {
            println!("  - {warning}");
        }
        Ok(ExitCode::FAILURE)
    }
}

fn dump_gc_audit(module: &WasmBinaryModule, _data: &[u8], opts: &SpyTargetOptions) -> Result<ExitCode> {
    let Some(bytes) = custom_section_bytes(module, "legion.wasm.gc_layouts")
    else {
        return Err(miette!("未找到 legion.wasm.gc_layouts 段；请用新版 legion 重新编译 wasm 目标"));
    };
    let text = String::from_utf8_lossy(&bytes);
    if opts.json {
        println!(
            "{{\"section\":\"legion.wasm.gc_layouts\",\"lines\":{}}}",
            serde_json::to_string(&text.lines().collect::<Vec<_>>()).unwrap_or_default()
        );
        return Ok(ExitCode::SUCCESS);
    }

    println!("=== wasm-gc 布局审计 (legion.wasm.gc_layouts) ===");
    let mut registered = 0usize;
    let mut missing = 0usize;
    let mut mir_uses = 0usize;
    let mut linear = 0usize;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("gc_required=") {
            println!("  {line}");
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        match cols.first().copied() {
            Some("struct") if cols.get(4) == Some(&"registered") => {
                registered += 1;
                println!(
                    "  [struct OK] id={} name={} type_index={} storage={}",
                    cols.get(1).unwrap_or(&"?"),
                    cols.get(2).unwrap_or(&"?"),
                    cols.get(3).unwrap_or(&"?"),
                    cols.get(5).unwrap_or(&"?")
                );
            }
            Some("struct") if cols.get(3) == Some(&"missing") => {
                missing += 1;
                println!(
                    "  [struct MISSING] id={} name={} reason={}",
                    cols.get(1).unwrap_or(&"?"),
                    cols.get(2).unwrap_or(&"?"),
                    cols.get(4).unwrap_or(&"?")
                );
            }
            Some("struct") if cols.get(4) == Some(&"linear_memory") => {
                linear += 1;
                println!("  [struct linear] id={} name={}", cols.get(1).unwrap_or(&"?"), cols.get(2).unwrap_or(&"?"));
            }
            Some("array") => {
                println!(
                    "  [array] key={} type_index={} status={}",
                    cols.get(1).unwrap_or(&"?"),
                    cols.get(2).unwrap_or(&"?"),
                    cols.get(3).unwrap_or(&"?")
                );
            }
            Some("mir_use") => {
                mir_uses += 1;
                println!(
                    "  [mir_use MISSING] layout_id={} type={} site={} in {}",
                    cols.get(1).unwrap_or(&"?"),
                    cols.get(2).unwrap_or(&"?"),
                    cols.get(3).unwrap_or(&"?"),
                    cols.get(4).unwrap_or(&"?")
                );
            }
            _ => println!("  {line}"),
        }
    }
    println!();
    println!("摘要: struct_registered={registered} struct_missing={missing} linear_memory={linear} mir_use_gaps={mir_uses}");
    if missing > 0 || mir_uses > 0 {
        println!("状态: FAIL — 存在未注册的 gc struct 布局");
    }
    else {
        println!("状态: OK — 闭包内引用 aggregate 均已注册 gc structtype");
    }
    Ok(ExitCode::SUCCESS)
}

// ========== Type 段结构化解析 ==========

/// 执行 Type 段结构化 dump。
fn dump_type_section(module: &WasmBinaryModule, _data: &[u8], opts: &SpyTargetOptions) -> Result<ExitCode> {
    let entries = parse_type_section(module);
    if opts.json {
        print_json_types(&entries);
        return Ok(ExitCode::SUCCESS);
    }
    if entries.is_empty() {
        println!("（无 Type 段）");
        return Ok(ExitCode::SUCCESS);
    }
    println!("=== Type 段（{} 项）===", entries.len());
    for entry in &entries {
        let detail = format_type_detail(&entry.kind);
        let offset_tag = format!("0x{:04X}", entry.file_offset);
        println!("  [{:>3}] @{} {:<11} {}", entry.index, offset_tag, entry.kind.name(), detail);
    }
    // 统计 arraytype 数量，便于诊断 V8 兼容性。
    let array_count = entries.iter().filter(|e| matches!(e.kind, WasmTypeKind::Array { .. })).count();
    if array_count > 0 {
        println!();
        println!("注意：检测到 {} 个 arraytype(0x61) 条目，Node.js v24 V8 不支持，会导致 `unknown type form: 97`。", array_count);
    }
    Ok(ExitCode::SUCCESS)
}

/// 格式化 type 条目内容。
fn format_type_detail(kind: &WasmTypeKind) -> String {
    match kind {
        WasmTypeKind::Func { params, results } => {
            let p = params.iter().map(wasm_value_type_name).collect::<Vec<_>>().join(", ");
            let r = results.iter().map(wasm_value_type_name).collect::<Vec<_>>().join(", ");
            format!("({}) -> ({})", p, r)
        }
        WasmTypeKind::Struct { fields } => {
            let f = fields
                .iter()
                .map(|(ty, m)| format!("{}{}", wasm_value_type_name(ty), if *m { "(mut)" } else { "" }))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{ {} }}", f)
        }
        WasmTypeKind::Array { element, mutable } => {
            format!("[{}] mut={}", wasm_value_type_name(element), mutable)
        }
        WasmTypeKind::Unknown { form } => format!("form=0x{:02X}", form),
    }
}

/// JSON 格式输出 type 段。
fn print_json_types(entries: &[WasmTypeEntry]) {
    let items: Vec<String> = entries
        .iter()
        .map(|e| {
            let detail = format_type_detail(&e.kind);
            format!(
                "    {{\"index\": {}, \"offset\": {}, \"kind\": {}, \"detail\": {}}}",
                e.index,
                e.file_offset,
                json_string(e.kind.name()),
                json_string(&detail)
            )
        })
        .collect();
    println!(
        r#"{{
  "count": {},
  "entries": [
{}
  ]
}}"#,
        entries.len(),
        items.join(",\n")
    );
}

// ========== 辅助输出 ==========

/// 格式化导入项的类型描述。
///
/// 按 `WasmExternalKind` 变体分发，与原 `spy` 本地 `WasmImport::type_str` 输出一致。
fn format_import_type(imp: &WasmImport) -> String {
    match &imp.kind {
        WasmExternalKind::Func => format!("type={}", imp.type_index),
        WasmExternalKind::Table => format!("elem={}", wasm_value_type_name(&imp.table_elem_type)),
        WasmExternalKind::Memory => match imp.memory_max {
            Some(max) => format!("limits={}..{}", imp.memory_min, max),
            None => format!("limits={}", imp.memory_min),
        },
        WasmExternalKind::Global => {
            format!("valtype={}, mut={}", wasm_value_type_name(&imp.global_value_type), imp.global_mutable)
        }
        WasmExternalKind::Unknown(_) => "?".to_string(),
    }
}

/// 将外部实体种类映射回原始字节值，用于 JSON 序列化。
fn external_kind_byte(kind: &WasmExternalKind) -> u8 {
    match kind {
        WasmExternalKind::Func => 0,
        WasmExternalKind::Table => 1,
        WasmExternalKind::Memory => 2,
        WasmExternalKind::Global => 3,
        WasmExternalKind::Unknown(b) => *b,
    }
}

/// 将 `block type` 的有符号整数表示映射为可读名称。
fn block_type_name(bt: i64) -> String {
    match bt {
        -64 => "void".to_string(),
        -1 => "i32".to_string(),
        -2 => "i64".to_string(),
        -3 => "f32".to_string(),
        -4 => "f64".to_string(),
        other => format!("type={}", other),
    }
}

/// 将解码后的操作数列表格式化为可读字符串，尽量贴近原 `spy` 输出风格。
///
/// 多个操作数以 `", "` 连接；空列表返回空字符串。
fn format_operands(operands: &[DecodedOperand]) -> String {
    let parts: Vec<String> = operands
        .iter()
        .map(|operand| match operand {
            DecodedOperand::TypeIndex(t) => format!("type={t}"),
            DecodedOperand::FieldIndex(f) => format!("field={f}"),
            DecodedOperand::Count(c) => format!("count={c}"),
            DecodedOperand::LocalIndex(l) => l.to_string(),
            DecodedOperand::GlobalIndex(g) => g.to_string(),
            DecodedOperand::LabelIndex(l) => l.to_string(),
            DecodedOperand::FuncIndex(f) => f.to_string(),
            DecodedOperand::SubOpcode(s) => s.to_string(),
            DecodedOperand::MemArg { align, offset } => format!("align={align}, offset={offset}"),
            DecodedOperand::BlockType(bt) => block_type_name(*bt),
            DecodedOperand::BrTargets { targets, default } => format!("targets={targets:?}, default={default}"),
            DecodedOperand::ValueI32(v) => v.to_string(),
            DecodedOperand::ValueI64(v) => v.to_string(),
            DecodedOperand::ValueF32(v) => v.to_string(),
            DecodedOperand::ValueF64(v) => v.to_string(),
            DecodedOperand::RefNull(r) => format!("0x{r:02X}"),
            DecodedOperand::RefFunc(f) => f.to_string(),
            DecodedOperand::SelectTypes(types) => format!("types={types:?}"),
            DecodedOperand::CallIndirect { type_idx, table_idx } => format!("type={type_idx}, table={table_idx}"),
        })
        .collect();
    parts.join(", ")
}

/// 打印指令列表。
fn print_instructions(instructions: &[DecodedInstruction]) {
    println!("=== 指令 ===");
    for instr in instructions {
        println!("  {:>4}:  {:<20} {}", instr.offset, instr.mnemonic, format_operands(&instr.operands));
    }
}

/// 打印 hex dump。
fn print_hex_dump(body: &[u8], func: &WasmFunctionEntry) {
    println!("=== 函数 {} 原始字节（{} 字节）===", func.index, body.len());
    print_hex_range(body, 0, body.len(), usize::MAX);
}

/// 打印 hex 范围，高亮指定偏移。
fn print_hex_range(data: &[u8], start: usize, end: usize, highlight: usize) {
    let mut offset = start;
    while offset < end {
        let chunk_end = (offset + 16).min(end);
        let chunk = &data[offset..chunk_end];

        let marker = if offset <= highlight && highlight < chunk_end { "*" } else { " " };
        print!("{} {:04X}: ", marker, offset);

        for (i, byte) in chunk.iter().enumerate() {
            if offset + i == highlight {
                print!("[{:02X}]", byte);
            }
            else if i % 8 == 0 && i > 0 {
                print!(" {:02X} ", byte);
            }
            else {
                print!(" {:02X}", byte);
            }
        }

        // 补齐对齐
        for _ in chunk.len()..16 {
            print!("   ");
        }

        print!("  |");
        for byte in chunk {
            let ch = *byte;
            if (32..127).contains(&ch) {
                print!("{}", ch as char);
            }
            else {
                print!(".");
            }
        }
        println!("|");

        offset = chunk_end;
    }
}

// ========== JSON 输出 ==========

fn print_json_overview(module: &WasmBinaryModule, data: &[u8]) {
    let sections: Vec<String> = module
        .sections
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let name = if s.id == SECTION_CUSTOM { s.name.clone().unwrap_or_default() } else { section_name(s.id).to_string() };
            format!("    {{\"index\": {}, \"id\": {}, \"name\": {}, \"size\": {}}}", i, s.id, json_string(&name), s.bytes.len())
        })
        .collect();

    let imports = parse_import_section(module);
    let imports_json: Vec<String> = imports
        .iter()
        .map(|i| {
            format!(
                "    {{\"module\": {}, \"field\": {}, \"kind\": {}, \"kind_str\": {}, \"type\": {}}}",
                json_string(&i.module),
                json_string(&i.field),
                external_kind_byte(&i.kind),
                json_string(i.kind.name()),
                json_string(&format_import_type(i))
            )
        })
        .collect();

    let exports = parse_export_section(module);
    let exports_json: Vec<String> = exports
        .iter()
        .map(|e| {
            format!(
                "    {{\"name\": {}, \"kind\": {}, \"kind_str\": {}, \"index\": {}}}",
                json_string(&e.name),
                external_kind_byte(&e.kind),
                json_string(e.kind.name()),
                e.index
            )
        })
        .collect();

    let functions = parse_code_section(module);
    let functions_json: Vec<String> = functions
        .iter()
        .map(|f| {
            format!(
                "    {{\"index\": {}, \"name\": {}, \"code_offset\": {}, \"body_len\": {}, \"local_groups\": {}}}",
                f.index,
                json_string(f.name.as_deref().unwrap_or("")),
                f.code_offset,
                f.body_len,
                f.local_groups
            )
        })
        .collect();

    println!(
        r#"{{
  "size": {},
  "version": {},
  "sections": [
{}
  ],
  "imports": [
{}
  ],
  "exports": [
{}
  ],
  "functions": [
{}
  ]
}}"#,
        data.len(),
        module.version,
        sections.join(",\n"),
        imports_json.join(",\n"),
        exports_json.join(",\n"),
        functions_json.join(",\n")
    );
}

fn print_json_function(func: &WasmFunctionEntry, instructions: &[DecodedInstruction]) {
    let json = serde_json::json!({
        "index": func.index,
        "name": func.name,
        "code_offset": func.code_offset,
        "body_len": func.body_len,
        "local_groups": func.local_groups,
        "instructions": instructions,
    });
    println!("{}", serde_json::to_string_pretty(&json).unwrap_or_default());
}

fn print_json_offset(offset: usize, func: Option<&WasmFunctionEntry>, _data: &[u8]) {
    match func {
        Some(f) => {
            println!(
                r#"{{
  "offset": {},
  "in_function": true,
  "function_index": {},
  "function_name": {},
  "function_range": [{}, {}]
}}"#,
                offset,
                f.index,
                json_string(f.name.as_deref().unwrap_or("")),
                f.code_offset,
                f.code_offset + f.body_len
            );
        }
        None => {
            println!(
                r#"{{
  "offset": {},
  "in_function": false
}}"#,
                offset
            );
        }
    }
}

/// JSON 字符串转义。
fn json_string(value: &str) -> String {
    let mut result = String::with_capacity(value.len() + 2);
    result.push('"');
    for ch in value.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => result.push_str(&format!("\\u{:04x}", c as u32)),
            c => result.push(c),
        }
    }
    result.push('"');
    result
}

/// Component-model binaries use magic `\0asm` with version `0x0d000001` (LE).
fn detect_component_version(data: &[u8]) -> Option<u32> {
    if data.len() < 8 || &data[0..4] != b"\0asm" {
        return None;
    }
    let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    // Core modules use version 1; component-model uses 0x0d000001.
    if version == 1 { None } else { Some(version) }
}

/// Dump a high-level overview of a WASI / component-model binary.
///
/// Full instruction disassembly still targets the paired `.core.wasm` (or a
/// nested core module extracted below when present as raw `\0asm` payload).
fn dump_component_overview(path: &str, data: &[u8], version: u32, opts: &SpyTargetOptions) -> Result<ExitCode> {
    let sections = scan_component_sections(data)?;
    let nested_cores = find_nested_core_modules_from_component(data, &sections);

    if opts.json {
        println!("{{");
        println!("  \"kind\": \"component\",");
        println!("  \"path\": {},", json_string(path));
        println!("  \"bytes\": {},", data.len());
        println!("  \"version\": {},", version);
        println!("  \"sections\": [");
        for (index, section) in sections.iter().enumerate() {
            let comma = if index + 1 == sections.len() { "" } else { "," };
            println!("    {{\"index\": {}, \"id\": {}, \"size\": {}, \"offset\": {}}}{comma}", index, section.id, section.size, section.offset);
        }
        println!("  ],");
        println!("  \"nested_core_modules\": [");
        for (index, core) in nested_cores.iter().enumerate() {
            let comma = if index + 1 == nested_cores.len() { "" } else { "," };
            println!("    {{\"offset\": {}, \"bytes\": {}}}{comma}", core.offset, core.bytes.len());
        }
        println!("  ]");
        println!("}}");
        return Ok(ExitCode::SUCCESS);
    }

    // Function disassembly short-circuits overview noise.
    if let Some(func_spec) = &opts.func {
        if nested_cores.is_empty() {
            return Err(miette!("component 中没有可反汇编的嵌套 core module"));
        }
        let core = &nested_cores[0];
        let module = WasmBinaryModule::from_bytes(&core.bytes).map_err(|error| miette!("嵌套 core 解析失败：{error}"))?;
        return disassemble_function(&module, &core.bytes, func_spec, opts);
    }

    println!("WASI component（{} 字节，version=0x{version:08x}）", data.len());
    println!("文件：{path}");
    println!();
    if let Some(wit_text) = dump_component_wit_via_wasm_tools(path) {
        println!("=== Component WIT（wasm-tools）===");
        for line in wit_text.lines().take(40) {
            println!("{line}");
        }
        if wit_text.lines().count() > 40 {
            println!("…（已截断）");
        }
        println!();
    }
    println!("=== Component 段列表 ===");
    for (index, section) in sections.iter().enumerate() {
        println!("  [{index}] id={} size={} offset=0x{:X}", section.id, section.size, section.offset);
    }
    println!();
    if nested_cores.is_empty() {
        println!("未在 component 载荷中发现嵌套 core module。");
        println!("提示：构建通常同时写出 `<name>.core.wasm`，可用 `legion spy wasm <name>.core.wasm --list` 反汇编。");
    }
    else {
        println!("=== 嵌套 core module ===");
        for (index, core) in nested_cores.iter().enumerate() {
            println!("  [{index}] offset=0x{:X} bytes={}", core.offset, core.bytes.len());
            match WasmBinaryModule::from_bytes(&core.bytes) {
                Ok(module) => {
                    let exports = parse_export_section(&module);
                    if !exports.is_empty() {
                        println!("    exports:");
                        for exp in &exports {
                            println!("      {} {} : index={}", exp.kind.name(), exp.name, exp.index);
                        }
                    }
                    if opts.list {
                        println!("    core 段数：{}", module.sections.len());
                        for (section_index, section) in module.sections.iter().enumerate() {
                            println!("      [{section_index}] {} size={}", section_name(section.id), section.bytes.len());
                        }
                    }
                }
                Err(error) => println!("    core 解析失败：{error}"),
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn dump_component_wit_via_wasm_tools(path: &str) -> Option<String> {
    let output = std::process::Command::new("wasm-tools").args(["component", "wit", path]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

#[derive(Debug, Clone, Copy)]
struct ComponentSectionRef {
    id: u8,
    size: usize,
    offset: usize,
}

#[derive(Debug, Clone)]
struct NestedCoreModule {
    offset: usize,
    bytes: Vec<u8>,
}

fn scan_component_sections(data: &[u8]) -> Result<Vec<ComponentSectionRef>> {
    if data.len() < 8 {
        return Err(miette!("component 文件过短"));
    }
    let mut cursor = 8usize;
    let mut sections = Vec::new();
    while cursor < data.len() {
        let id = data[cursor];
        cursor += 1;
        let (size, size_len) = read_uleb128_at(data, cursor)?;
        cursor += size_len;
        let size = size as usize;
        if cursor + size > data.len() {
            return Err(miette!("component 段越界：id={id} size={size} offset=0x{cursor:X}"));
        }
        sections.push(ComponentSectionRef { id, size, offset: cursor });
        cursor += size;
    }
    Ok(sections)
}

fn find_nested_core_modules_from_component(data: &[u8], sections: &[ComponentSectionRef]) -> Vec<NestedCoreModule> {
    // Component section id 1 is a core `module` whose payload is a complete `\0asm` binary.
    let mut found = Vec::new();
    for section in sections {
        if section.id != 1 {
            continue;
        }
        let end = section.offset.saturating_add(section.size);
        if end > data.len() || section.size < 8 {
            continue;
        }
        let bytes = data[section.offset..end].to_vec();
        if &bytes[0..4] != b"\0asm" {
            continue;
        }
        let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        if version != 1 {
            continue;
        }
        if WasmBinaryModule::from_bytes(&bytes).is_ok() {
            found.push(NestedCoreModule { offset: section.offset, bytes });
        }
    }
    if !found.is_empty() {
        return found;
    }
    // Fallback: scan for embedded `\0asm` version-1 payloads (best-effort).
    find_nested_core_modules(data)
}

fn find_nested_core_modules(data: &[u8]) -> Vec<NestedCoreModule> {
    let mut found = Vec::new();
    let mut index = 0usize;
    while index + 8 <= data.len() {
        if &data[index..index + 4] == b"\0asm" {
            let version = u32::from_le_bytes([data[index + 4], data[index + 5], data[index + 6], data[index + 7]]);
            if version == 1 {
                if let Some(end) = measure_core_module_end(&data[index..]) {
                    found.push(NestedCoreModule { offset: index, bytes: data[index..index + end].to_vec() });
                    index += end;
                    continue;
                }
            }
        }
        index += 1;
    }
    found
}

fn measure_core_module_end(data: &[u8]) -> Option<usize> {
    if data.len() < 8 || &data[0..4] != b"\0asm" {
        return None;
    }
    let mut cursor = 8usize;
    while cursor < data.len() {
        let id = *data.get(cursor)?;
        // Core module section ids are 0..=12; component sections use other ids.
        if id > 12 {
            return Some(cursor);
        }
        cursor += 1;
        let (size, size_len) = read_uleb128_at(data, cursor).ok()?;
        cursor += size_len;
        let size = size as usize;
        cursor = cursor.checked_add(size)?;
        if cursor > data.len() {
            return None;
        }
    }
    Some(cursor)
}

fn read_uleb128_at(data: &[u8], start: usize) -> Result<(u64, usize)> {
    let mut result = 0u64;
    let mut shift = 0u32;
    let mut consumed = 0usize;
    loop {
        let byte = *data.get(start + consumed).ok_or_else(|| miette!("uleb128 越界 @ {}", start + consumed))?;
        consumed += 1;
        result |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok((result, consumed));
        }
        shift += 7;
        if shift > 63 {
            return Err(miette!("uleb128 过长 @ {start}"));
        }
    }
}
