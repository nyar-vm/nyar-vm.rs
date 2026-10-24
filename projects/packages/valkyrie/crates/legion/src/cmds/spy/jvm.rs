//! legion spy jvm 子模式：JVM class 文件解析与字节码反汇编。
//!
//! 支持 `.class` 和 `.jar` 文件，可列出方法签名、反汇编指定方法体。
//! 用于定位 JVM 字节码验证错误。
//!
//! 二进制解析与反汇编能力由 `acorn-jvm` 提供，本模块仅负责命令行入口与纯文本渲染。

use std::{fs, path::Path, process::ExitCode};

use acorn_jvm::{
    class::{ConstantPool, DecodedJvmOperand, JvmClassFile, JvmFieldRef, JvmMethodRef, decode_instructions},
    jar::JvmJarPackage,
};
use miette::{IntoDiagnostic, Result, miette};

use super::{SpyOptions, SpyTargetOptions};

/// 执行 JVM 字节码 dump。
///
/// 支持的文件扩展名：`.class` / `.jar`。
pub fn run(options: &SpyOptions) -> Result<ExitCode> {
    let (_, options) = options.split();
    let Some(target) = &options.input
    else {
        return Err(miette!(
            r#"用法：legion spy jvm <file> [--method <name>] [--func <class>] [--list] [--json]
  file               目标文件（.class / .jar）
  --method <name>    输出包含指定名称的方法体
  --func <class>     仅输出 JAR 中匹配的 class（内部名或二进制名片段）
  --list             列出所有方法签名（JAR 时先列类名）
  --json             以 JSON 格式输出"#
        ));
    };

    if !Path::exists(Path::new(target)) {
        return Err(miette!("文件不存在：{}", target));
    }

    let extension = Path::new(target).extension().and_then(|ext| ext.to_str()).map(|ext| ext.to_ascii_lowercase()).unwrap_or_default();

    match extension.as_str() {
        "class" => run_class_file(target, options),
        "jar" => run_jar_file(target, options),
        other => Err(miette!("不支持的文件扩展名 '{}'，支持 .class / .jar", other)),
    }
}

/// 分析单个 `.class` 文件。
fn run_class_file(target: &str, options: &SpyTargetOptions) -> Result<ExitCode> {
    let data = fs::read(target).into_diagnostic().map_err(|error| error.wrap_err(format!("无法读取文件 {}", target)))?;
    let class = JvmClassFile::from_bytes(&data).map_err(|error| miette!("class 解析失败：{}", error))?;
    output_class(&class, options);
    Ok(ExitCode::SUCCESS)
}

/// 分析 `.jar` 文件，遍历其中所有 `.class` 条目。
fn run_jar_file(target: &str, options: &SpyTargetOptions) -> Result<ExitCode> {
    let data = fs::read(target).into_diagnostic().map_err(|error| error.wrap_err(format!("无法读取文件 {}", target)))?;
    let package = JvmJarPackage::from_bytes(target, &data).map_err(|error| miette!("JAR 解析失败：{}", error))?;

    println!("jar: {}", target);
    match &package.main_class {
        Some(main_class) => println!("Main-Class: {}", main_class),
        None => println!("Main-Class: (缺失)"),
    }
    let class_entries: Vec<_> = package.iter_classes().collect();
    println!("classes: {}", class_entries.len());
    if options.list {
        println!("\n--- 类列表 ---");
        for (name, _) in &class_entries {
            println!("  {}", name);
        }
    }

    let class_filter = options.func.as_deref();
    for (name, class_data) in class_entries {
        if let Some(filter) = class_filter {
            let internal = name.trim_end_matches(".class");
            if !(internal.contains(filter) || name.contains(filter) || internal.replace('/', ".").contains(filter)) {
                continue;
            }
        }
        let Ok(class) = JvmClassFile::from_bytes(class_data)
        else {
            eprintln!("警告：无法解析 class 条目 {}", name);
            continue;
        };
        if options.method.is_some() {
            if let Some(method_name) = options.method.as_deref() {
                if class.methods.iter().any(|m| m.name.contains(method_name)) {
                    println!("=== {} ===", name);
                    output_class(&class, options);
                }
            }
        }
        else if options.list {
            println!("=== {} ===", name);
            output_class(&class, options);
        }
        else {
            println!("=== {} ===", name);
            output_class(&class, options);
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// 输出 class 文件信息。
fn output_class(class: &JvmClassFile, options: &SpyTargetOptions) {
    if options.json {
        output_class_json(class, options);
        return;
    }

    println!("class: {}", class.internal_name);
    println!("super: {}", class.super_name);
    println!("version: {}.{}", class.major_version, class.minor_version);
    println!("methods: {}", class.methods.len());

    if options.list {
        println!("\n--- 方法列表 ---");
        for method in &class.methods {
            println!("  {}{}", method.name, method.descriptor);
        }
        return;
    }

    if let Some(method_name) = &options.method {
        for (index, method) in class.methods.iter().enumerate() {
            if method.name.contains(method_name) {
                println!("\n--- {}{} ---", method.name, method.descriptor);
                if let Some(code) = class.raw_method_code.get(index).and_then(|c| c.as_ref()) {
                    println!("code length: {} bytes", code.len());
                    disassemble(code, &class.constant_pool);
                }
                else {
                    println!("(无方法体 - native 或 abstract)");
                }
            }
        }
        return;
    }

    println!("\n--- 方法签名 ---");
    for method in &class.methods {
        let flags = format_access_flags(method.access_flags);
        println!("  {} {}{}", flags, method.name, method.descriptor);
    }
}

/// 以 JSON 格式输出 class 文件信息。
fn output_class_json(class: &JvmClassFile, options: &SpyTargetOptions) {
    if options.list {
        let json = serde_json::to_string_pretty(&class.methods).unwrap_or_else(|_| "[]".to_string());
        println!("{}", json);
        return;
    }

    if let Some(method_name) = &options.method {
        for (index, method) in class.methods.iter().enumerate() {
            if !method.name.contains(method_name) {
                continue;
            }
            if let Some(code) = class.raw_method_code.get(index).and_then(|c| c.as_ref()) {
                let instructions = decode_instructions(code, &class.constant_pool);
                let json = serde_json::to_string_pretty(&instructions).unwrap_or_else(|_| "[]".to_string());
                println!("{}", json);
            }
        }
        return;
    }

    let json = serde_json::to_string_pretty(class).unwrap_or_else(|_| "{}".to_string());
    println!("{}", json);
}

/// 格式化访问标志。
fn format_access_flags(flags: u16) -> String {
    let mut parts = Vec::new();
    if flags & 0x0001 != 0 {
        parts.push("public");
    }
    if flags & 0x0002 != 0 {
        parts.push("private");
    }
    if flags & 0x0004 != 0 {
        parts.push("protected");
    }
    if flags & 0x0008 != 0 {
        parts.push("static");
    }
    if flags & 0x0010 != 0 {
        parts.push("final");
    }
    if flags & 0x0400 != 0 {
        parts.push("abstract");
    }
    if flags & 0x0100 != 0 {
        parts.push("native");
    }
    parts.join(" ")
}

// ============ 字节码反汇编 ============

/// 反汇编字节码并输出。
fn disassemble(code: &[u8], constant_pool: &ConstantPool) {
    let instructions = decode_instructions(code, constant_pool);
    for instruction in &instructions {
        let operand_str = format_operand(instruction.offset, &instruction.operand, constant_pool);
        println!("  {:4}: {:<12} {}", instruction.offset, instruction.mnemonic, operand_str);
    }
}

fn instruction_abs_target(pc: usize, relative: i32) -> i32 {
    i32::try_from(pc).unwrap_or(i32::MAX).saturating_add(relative)
}

/// 将结构化操作数渲染为人类可读文本。
fn format_operand(instruction_offset: usize, operand: &Option<DecodedJvmOperand>, pool: &ConstantPool) -> String {
    let Some(operand) = operand
    else {
        return String::new();
    };
    match operand {
        DecodedJvmOperand::ConstantIndex(index) => {
            let value = pool
                .string_value(*index)
                .or_else(|| pool.integer(*index).map(|v| v.to_string()))
                .or_else(|| pool.long(*index).map(|v| v.to_string()))
                .or_else(|| pool.float(*index).map(|v| v.to_string()))
                .or_else(|| pool.double(*index).map(|v| v.to_string()))
                .unwrap_or_else(|| format!("#{}", index));
            format!("#{} ({})", index, value)
        }
        DecodedJvmOperand::Branch(offset) => {
            // `offset` 是相对当前指令起始地址的有符号位移；同时打印绝对 PC 便于对照 VerifyError。
            format!("-> {offset:+} (abs {})", instruction_abs_target(instruction_offset, *offset))
        }
        DecodedJvmOperand::BranchWide(offset) => {
            format!("-> {offset:+} (abs {})", instruction_abs_target(instruction_offset, *offset))
        }
        DecodedJvmOperand::MethodRef(method_ref) => format_method_ref(method_ref),
        DecodedJvmOperand::FieldRef(field_ref) => format_field_ref(field_ref),
        DecodedJvmOperand::ClassRef(name) => name.clone(),
        DecodedJvmOperand::Int(value) => value.to_string(),
        DecodedJvmOperand::Long(value) => value.to_string(),
        DecodedJvmOperand::Float(value) => value.to_string(),
        DecodedJvmOperand::Double(value) => value.to_string(),
        DecodedJvmOperand::Local(index) => index.to_string(),
        DecodedJvmOperand::TableSwitch { default, low, high, offsets } => {
            format!("default->{} low={} high={} cases={:?}", default, low, high, offsets)
        }
        DecodedJvmOperand::LookupSwitch { default, pairs } => {
            format!("default->{} pairs={:?}", default, pairs)
        }
        DecodedJvmOperand::InvokeInterface { method, count } => {
            format!("{} count={}", format_method_ref(method), count)
        }
        DecodedJvmOperand::MultiANewArray { class, dimensions } => {
            format!("{} dims={}", class, dimensions)
        }
        DecodedJvmOperand::WideLocal(index) => index.to_string(),
    }
}

/// 将方法引用渲染为 `owner.name descriptor` 文本。
fn format_method_ref(method_ref: &JvmMethodRef) -> String {
    format!("{}.{}{}", method_ref.owner, method_ref.name, method_ref.descriptor)
}

/// 将字段引用渲染为 `owner.name descriptor` 文本。
fn format_field_ref(field_ref: &JvmFieldRef) -> String {
    format!("{}.{} {}", field_ref.owner, field_ref.name, field_ref.descriptor)
}
