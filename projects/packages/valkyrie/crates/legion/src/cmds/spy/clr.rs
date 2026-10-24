//! legion spy clr 子模式：CLR / MSIL dump 与分析。
//!
//! 支持两种分析路径：
//! 1. 直接 `PE` 二进制解析（默认，跨平台，不依赖 `ildasm`）。
//! 2. `ildasm` 文本转换（可选，仅 `Windows` 且 `ildasm` 可用时）。
//!
//! 对于 `.msil` / `.il` 文本，直接使用 `MsilParser` 解析方法体。
//!
//! `PE/CLR` 二进制解析与 `MSIL` 文本解析模型统一由 `acorn-pe` 提供，
//! 本模块仅负责命令行入口与纯文本渲染。

use std::{
    fs,
    path::Path,
    process::{Command, ExitCode},
};

use acorn_pe::{
    msil::{IlOperand, MsilParser, MsilTextMethod},
    pe::{
        CliHeader, IlInstruction, MetadataRoot, SectionHeader, parse_method_body_il, parse_pe, read_idx, read_strings_string, read_u32_table,
        rva_to_offset, table_data_offset,
    },
};
use miette::{IntoDiagnostic, Result, miette};

use super::{SpyOptions, SpyTargetOptions, pe_dump};

/// 执行 CLR / MSIL dump。
///
/// 支持的文件扩展名：`.msil` / `.il` / `.exe` / `.dll`。
/// 对于 `.exe` / `.dll`，默认使用直接 `PE` 解析（跨平台）。
pub fn run(options: &SpyOptions) -> Result<ExitCode> {
    let (_, options) = options.split();
    let Some(target) = &options.input
    else {
        return Err(miette!(
            r#"用法：legion spy clr <file> [--method <name>] [--list] [--json]
  file               目标文件（.msil / .il / .exe / .dll）
  --method <name>    输出包含指定名称的方法体
  --list             列出所有方法签名
  --json             以 JSON 格式输出"#
        ));
    };

    if !Path::exists(Path::new(target)) {
        return Err(miette!("文件不存在：{}", target));
    }

    let extension = Path::new(target).extension().and_then(|ext| ext.to_str()).map(|ext| ext.to_ascii_lowercase()).unwrap_or_default();

    match extension.as_str() {
        "msil" | "il" => run_msil_text(target, options),
        "exe" | "dll" => run_pe_binary(target, options),
        other => Err(miette!("不支持的文件扩展名 '{}'，支持 .msil / .il / .exe / .dll", other)),
    }
}

/// 分析 `PE` 二进制（`.exe` / `.dll`）。
///
/// 默认使用直接 `PE` 解析；若 `ILDASM_PATH` 设置且 `--ildasm` 标志启用，则使用 `ildasm`。
fn run_pe_binary(target: &str, options: &SpyTargetOptions) -> Result<ExitCode> {
    let data = fs::read(target).into_diagnostic().map_err(|error| error.wrap_err(format!("无法读取文件 {}", target)))?;

    match parse_pe(&data) {
        Ok(image) => {
            let dump = pe_dump::dump_image(&image);
            println!("{}", dump);
            if options.list {
                if let Some(md) = &image.metadata {
                    println!("{}", pe_dump::dump_user_strings(md));
                    println!("{}", pe_dump::dump_blobs(md));
                }
            }
            // 如果指定了方法名，尝试反汇编该方法体。
            if let (Some(method_name), Some(md)) = (&options.method, &image.metadata) {
                if let Some(cli) = &image.cli {
                    disassemble_method(&data, &image.sections, method_name, cli, md);
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(error) => Err(miette!("PE 解析失败：{}\n提示：若需使用 ildasm 文本路径，请设置 ILDASM_PATH 环境变量", error)),
    }
}

/// 反汇编指定名称的方法体。
fn disassemble_method(data: &[u8], sections: &[SectionHeader], method_name: &str, _cli: &CliHeader, md: &MetadataRoot) {
    // 查找匹配的方法名。
    let methoddef_rows = md.row_counts[0x06 as usize]; // MethodDef
    let strings_idx_size = if md.strings.len() > 0xFFFF { 4 } else { 2 };
    let blob_idx_size = if md.blob.len() > 0xFFFF { 4 } else { 2 };

    // 计算 MethodDef 行大小。
    let param_rows = md.row_counts[0x08 as usize]; // Param
    let param_idx_size = if param_rows > 0xFFFF { 4 } else { 2 };
    let row_size = 4 + 2 + 2 + strings_idx_size + blob_idx_size + param_idx_size;

    for i in 0..methoddef_rows {
        // 从 table_sizes 获取正确的偏移。
        let cursor = table_data_offset(md, 0x06).unwrap_or(0);
        let row_start = cursor + (i as usize) * row_size;
        if row_start + row_size > md.tables.len() {
            continue;
        }

        let mut off = row_start + 8; // 跳过 RVA, ImplFlags, Flags
        let name_idx = read_idx(&md.tables, off, strings_idx_size);
        let name = read_strings_string(&md.strings, name_idx);

        if !name.contains(method_name) {
            continue;
        }

        off += strings_idx_size;
        let sig_idx = read_idx(&md.tables, off, blob_idx_size);

        // 从 MethodDef 表获取 RVA。
        let rva = read_u32_table(&md.tables, row_start);

        println!("\n=== 方法体反汇编: {} (Row {}) ===", name, i + 1);
        println!("  RVA: 0x{:08X}", rva);
        println!("  SigBlob: 0x{:08X}", sig_idx);

        // 从 .text 节读取方法体原始字节。
        let file_offset = match rva_to_offset(sections, rva) {
            Ok(offset) => offset as usize,
            Err(_) => {
                println!("  错误：无法解析 RVA 0x{:08X}", rva);
                return;
            }
        };

        if file_offset >= data.len() {
            println!("  错误：文件偏移越界");
            return;
        }

        // 解析方法体 header。
        if file_offset + 1 >= data.len() {
            println!("  错误：数据不足");
            return;
        }

        let header_byte = data[file_offset];
        let is_tiny = (header_byte & 0x03) == 0x02;

        if is_tiny {
            let code_len_bytes = (header_byte >> 2) as usize;
            println!("  Format: Tiny ({} bytes)", code_len_bytes);

            let il_data = &data[file_offset + 1..file_offset + 1 + code_len_bytes as usize];
            let instructions = parse_method_body_il(il_data, true);
            print_il_instructions(&instructions);
        }
        else {
            if file_offset + 12 >= data.len() {
                println!("  错误：Fat header 数据不足");
                return;
            }
            let flags = u16::from_le_bytes([data[file_offset], data[file_offset + 1]]);
            let format = flags & 0x03;
            if format != 0x03 {
                println!("  错误：未知格式 0x{:02X}", format);
                return;
            }
            let max_stack = u16::from_le_bytes([data[file_offset + 2], data[file_offset + 3]]);
            let code_len =
                u32::from_le_bytes([data[file_offset + 4], data[file_offset + 5], data[file_offset + 6], data[file_offset + 7]]) as usize;

            // LocalVarSigTok 在字节 8-11，是元数据 token，不是长度前缀 blob。
            // IL 代码从偏移 12 开始 (ECMA-335 II.25.4.3)。
            println!("  Format: Fat (max_stack={}, code_len={})", max_stack, code_len);

            // 传递完整方法体（含 header）给 parse_method_body_il，让它自行解析 header。
            let body_data = &data[file_offset..file_offset + 12 + code_len];
            let instructions = parse_method_body_il(body_data, false);
            print_il_instructions(&instructions);
        }

        println!();
    }
}

/// 打印 IL 指令列表。
fn print_il_instructions(instructions: &[IlInstruction]) {
    for instr in instructions {
        let operand = format_il_operand(&instr.operand);
        let line = format!("    {:>4}:  {}\t{}", instr.offset, instr.opcode, operand);
        println!("{}", line);
    }
}

/// 将结构化 `IL` 操作数渲染为人类可读文本。
fn format_il_operand(operand: &IlOperand) -> String {
    match operand {
        IlOperand::None => String::new(),
        IlOperand::ShortBrTarget(target) | IlOperand::BrTarget(target) => format!("0x{:X}", target),
        IlOperand::FieldToken(token)
        | IlOperand::MethodToken(token)
        | IlOperand::TypeToken(token)
        | IlOperand::StringToken(token)
        | IlOperand::Token(token) => format!("0x{:08X}", token),
        IlOperand::Byte(value) => value.to_string(),
        IlOperand::Short(value) => value.to_string(),
        IlOperand::Int8(value) => value.to_string(),
        IlOperand::Int32(value) => value.to_string(),
        IlOperand::Int64(value) => value.to_string(),
        IlOperand::Float32(value) => value.to_string(),
        IlOperand::Float64(value) => value.to_string(),
        IlOperand::Switch(targets) => format!("{:?}", targets),
    }
}

/// 分析 `MSIL` 文本（`.msil` / `.il`）。
fn run_msil_text(target: &str, options: &SpyTargetOptions) -> Result<ExitCode> {
    let content = fs::read_to_string(target).into_diagnostic().map_err(|error| error.wrap_err(format!("无法读取文件 {}", target)))?;

    let methods = MsilParser::parse_methods(&content);

    // 未指定任何过滤条件时，默认等价于 --list
    let effective_list = options.list || options.method.is_none();

    if effective_list {
        print_list(&methods, options.json);
        return Ok(ExitCode::SUCCESS);
    }

    let method = options.method.as_deref().unwrap_or_default();
    print_method_block(&methods, method, options.json)
}

/// 尝试通过 `ildasm` 将 `.exe` / `.dll` 转为 MSIL 文本（可选路径）。
///
/// 成功时返回临时 MSIL 文件路径。失败时返回结构化诊断。
#[allow(dead_code)]
fn convert_with_ildasm(file: &str) -> Result<String> {
    let ildasm = resolve_ildasm().ok_or_else(|| {
        miette!(
            r#"ildasm 不可用。
请先转换为文本格式：ildasm "{}" /text > "{}.msil"
然后运行：legion spy clr "{}.msil" --method <name>"#,
            file,
            file,
            file
        )
    })?;

    let output = Command::new(&ildasm)
        .arg(file)
        .arg("/text")
        .arg("/utf8")
        .output()
        .into_diagnostic()
        .map_err(|error| error.wrap_err("无法启动 ildasm"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(miette!(
            r#"ildasm 退出码 {}。
{}"#,
            output.status.code().unwrap_or(-1),
            stderr
        ));
    }

    let temp_dir = std::env::temp_dir();
    let temp_name = format!("{}_{}.msil", Path::new(file).file_stem().and_then(|s| s.to_str()).unwrap_or("output"), uuid_like());
    let temp_path = temp_dir.join(temp_name);

    fs::write(&temp_path, &output.stdout).into_diagnostic().map_err(|error| error.wrap_err("无法写入临时文件"))?;

    Ok(temp_path.to_string_lossy().into_owned())
}

/// 生成一个简单的伪 UUID 字符串，用于临时文件命名。
fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:016x}", nanos)
}

/// 解析 `ILDASM_PATH` 环境变量或在 `PATH` 中查找 `ildasm`。
fn resolve_ildasm() -> Option<String> {
    if let Ok(path) = std::env::var("ILDASM_PATH") {
        if !path.trim().is_empty() && Path::new(&path).exists() {
            return Some(path);
        }
    }

    find_on_path("ildasm.exe").or_else(|| find_on_path("ildasm"))
}

/// 在 `PATH` 中查找指定可执行文件。
fn find_on_path(file_name: &str) -> Option<String> {
    let path_var = std::env::var("PATH").ok()?;
    let separator = if cfg!(windows) { ';' } else { ':' };

    for dir in path_var.split(separator) {
        let dir = dir.trim_matches('"');
        if dir.is_empty() {
            continue;
        }
        let candidate = Path::new(dir).join(file_name);
        if candidate.exists() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }

    None
}

/// 输出方法列表（文本或 JSON）。
fn print_list(methods: &[MsilTextMethod], json: bool) {
    if json {
        print_json_list(methods);
        return;
    }

    if methods.is_empty() {
        println!("（未发现任何 .method）");
        return;
    }

    println!("共 {} 个方法：", methods.len());
    for method in methods {
        println!("  [{:>5}]  {}", method.start_line, method.signature);
    }
}

/// 输出匹配指定名称的方法体。
fn print_method_block(methods: &[MsilTextMethod], method: &str, json: bool) -> Result<ExitCode> {
    let matches: Vec<&MsilTextMethod> = methods
        .iter()
        .filter(|m| {
            let name_lower = m.name.to_ascii_lowercase();
            let sig_lower = m.signature.to_ascii_lowercase();
            let method_lower = method.to_ascii_lowercase();
            name_lower.contains(&method_lower) || sig_lower.contains(&method_lower)
        })
        .collect();

    if matches.is_empty() {
        return Err(miette!("未找到匹配 '{}' 的方法", method));
    }

    if json {
        print_json_methods(&matches);
        return Ok(ExitCode::SUCCESS);
    }

    for method in &matches {
        for line in &method.body {
            println!("{}", line);
        }
        println!();
    }

    println!("共匹配 {} 个方法。", matches.len());
    Ok(ExitCode::SUCCESS)
}

/// 以 JSON 格式输出方法列表。
fn print_json_list(methods: &[MsilTextMethod]) {
    let items: Vec<String> = methods
        .iter()
        .map(|m| {
            format!(
                r#"    {{
      "signature": {},
      "name": {},
      "line": {},
      "size": {}
    }}"#,
                json_string(&m.signature),
                json_string(&m.name),
                m.start_line,
                m.body.len()
            )
        })
        .collect();

    println!("[\n{}\n]", items.join(",\n"));
}

/// 以 JSON 格式输出方法体。
fn print_json_methods(matches: &[&MsilTextMethod]) {
    let items: Vec<String> = matches
        .iter()
        .map(|m| {
            let body = m.body.join("\n");
            format!(
                r#"    {{
      "signature": {},
      "name": {},
      "line": {},
      "body": {}
    }}"#,
                json_string(&m.signature),
                json_string(&m.name),
                m.start_line,
                json_string(&body)
            )
        })
        .collect();

    println!("[\n{}\n]", items.join(",\n"));
}

/// 将字符串转义为 JSON 字符串字面量（含两侧双引号）。
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
