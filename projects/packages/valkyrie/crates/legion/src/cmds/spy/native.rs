//! legion spy native 子模式：Native x86-64 指令序列反汇编与诊断。
//!
//! 支持两种输入路径：
//! 1. `X64Instruction` JSON dump（由 native lowering 在设置
//!    `VALKYRIE_NATIVE_DUMP=<path>` 环境变量时写出）。该路径可完整还原
//!    `Label` / `CallLabel` / `Je` / `Jne` / `Jmp` 的目标，以及
//!    `MovRspOffsetReg32` 等 IR 级指令，便于定位 access violation 根因。
//! 2. 原始 `.text` 字节反汇编（配合 `--hex` 选项查看字节级编码）。
//!
//! 设计目标：当 native 后端产出的 PE/ELF 在运行时崩溃（例如 access violation
//! `-1073741819`），开发者可通过设置 `VALKYRIE_NATIVE_DUMP` 环境变量重新
//! 编译，再用 `legion spy native <dump.json>` 查看 lowering 阶段实际发射的
//! 指令序列，精确定位崩溃点。

use std::{fs, path::Path, process::ExitCode};

use acorn_x64::x86_64::{ConditionCode, Reg64, X64Instruction};
use miette::{IntoDiagnostic, Result, miette};

use super::SpyOptions;

/// JSON dump 文件的顶层结构。
///
/// 序列化由 `emitter` 在 `lower_windows_pe` / `lower_linux_elf` 中完成，
/// 反序列化由本模块完成。字段命名与 `X64Instruction` 变体一一对应，便于
/// 手工核对。
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct NativeDump {
    /// 产生该 dump 的目标三元式 / host flavor。
    pub host_flavor: String,
    /// 入口符号名。
    pub entry_symbol: String,
    /// 栈预留量（字节）。
    pub stack_reserve: u32,
    /// 指令序列（直接复用 `X64Instruction`，通过 serde 序列化）。
    pub instructions: Vec<X64Instruction>,
}

/// 执行 native x86-64 反汇编。
///
/// 支持的文件扩展名：`.json`（`X64Instruction` JSON dump）。
pub fn run(options: &SpyOptions) -> Result<ExitCode> {
    let (_, opts) = options.split();
    let Some(input) = &opts.input
    else {
        return Err(miette!(
            r#"用法：legion spy native <file.json> [--func <name>] [--list] [--json]
  file               X64Instruction JSON dump 路径
  --func <name>      仅输出包含指定名称的 label 区段
  --list             列出所有 label 及其偏移
  --json             以 JSON 格式输出"#
        ));
    };

    if !Path::exists(Path::new(input)) {
        return Err(miette!("文件不存在：{}", input));
    }

    let content = fs::read_to_string(input).into_diagnostic()?;
    let dump: NativeDump = serde_json::from_str(&content).map_err(|e| miette!("JSON 解析失败：{e}"))?;

    if opts.list {
        list_labels(&dump);
        return Ok(ExitCode::SUCCESS);
    }

    if opts.json {
        let json = serde_json::to_string_pretty(&dump).map_err(|e| miette!("JSON 序列化失败：{e}"))?;
        println!("{json}");
        return Ok(ExitCode::SUCCESS);
    }

    let filter = opts.func.as_deref();
    disassemble(&dump, filter);
    Ok(ExitCode::SUCCESS)
}

/// 列出所有 label 及其在指令序列中的索引。
fn list_labels(dump: &NativeDump) {
    println!("=== Labels (entry={})", dump.entry_symbol);
    println!("  stack_reserve = {} (0x{:X})", dump.stack_reserve, dump.stack_reserve);
    println!("  host_flavor   = {}", dump.host_flavor);
    println!();
    println!("  {:<6} {:<8} {}", "index", "offset", "label");
    let mut offset: u32 = 0;
    for (i, ins) in dump.instructions.iter().enumerate() {
        if let X64Instruction::Label(name) = ins {
            println!("  {:<6} 0x{:06X} {}", i, offset, name);
        }
        offset += estimate_instruction_size(ins);
    }
}

/// 反汇编指令序列，可选地按 label 名过滤区段。
fn disassemble(dump: &NativeDump, filter: Option<&str>) {
    println!("=== Native Disassembly (entry={}, host={})", dump.entry_symbol, dump.host_flavor);
    println!("  stack_reserve = {} (0x{:X})", dump.stack_reserve, dump.stack_reserve);
    println!();

    let mut offset: u32 = 0;
    let mut in_filtered_section = filter.is_none();
    for ins in &dump.instructions {
        if let X64Instruction::Label(name) = ins {
            if let Some(f) = filter {
                in_filtered_section = name.contains(f);
            }
            if in_filtered_section {
                println!("\n{name}:");
            }
        }
        else if in_filtered_section {
            let asm = render_instruction(ins);
            println!("  0x{:06X}: {}", offset, asm);
        }
        offset += estimate_instruction_size(ins);
    }
}

/// 渲染单条指令为 AT&T 风格汇编文本。
fn render_instruction(ins: &X64Instruction) -> String {
    match ins {
        X64Instruction::Label(name) => format!("{name}:"),
        X64Instruction::SubRsp(imm) => format!("sub rsp, 0x{imm:X}"),
        X64Instruction::AddRsp(imm) => format!("add rsp, 0x{imm:X}"),
        X64Instruction::MovRegImm32(reg, imm) => format!("mov {}, 0x{imm:X}", reg_name(reg)),
        X64Instruction::MovRegImm64(reg, imm) => format!("movabs {}, 0x{imm:X}", reg_name(reg)),
        X64Instruction::MovRegReg { dst, src } => format!("mov {}, {}", reg_name(dst), reg_name(src)),
        X64Instruction::XorReg { dst, src } => format!("xor {}, {}", reg_name(dst), reg_name(src)),
        X64Instruction::LeaRipRelative { dst, label } => format!("lea {}, [rip + {label}]", reg_name(dst)),
        X64Instruction::CallImport { slot } => format!("call [rip + import_{slot}]"),
        X64Instruction::MovStackArgQword { value } => format!("mov qword [rsp+0x20], 0x{value:X}"),
        X64Instruction::LeaRspOffset { dst, offset } => format!("lea {}, [rsp + 0x{offset:X}]", reg_name(dst)),
        X64Instruction::MovRegMemReg { dst, base, offset } => {
            format!("mov {}, [{} + 0x{offset:X}]", reg_name(dst), reg_name(base))
        }
        X64Instruction::MovRegRspOffset { dst, offset } => format!("mov {}, [rsp + 0x{offset:X}]", reg_name(dst)),
        X64Instruction::MovRspOffsetReg32 { offset, src } => {
            format!("mov qword [rsp + 0x{offset:X}], {}", reg_name(src))
        }
        X64Instruction::MovMemRegImm32 { base, offset, value } => {
            format!("mov dword [{} + 0x{offset:X}], 0x{value:X}", reg_name(base))
        }
        X64Instruction::MovRspOffsetImm32 { offset, value } => {
            format!("mov dword [rsp + 0x{offset:X}], 0x{value:X}")
        }
        X64Instruction::AddRegReg { dst, src } => format!("add {}, {}", reg_name(dst), reg_name(src)),
        X64Instruction::SubRegReg { dst, src } => format!("sub {}, {}", reg_name(dst), reg_name(src)),
        X64Instruction::ImulRegReg { dst, src } => format!("imul {}, {}", reg_name(dst), reg_name(src)),
        X64Instruction::IdivReg { divisor } => format!("cqo; idiv {}", reg_name(divisor)),
        X64Instruction::CmpRegReg { dst, src } => format!("cmp {}, {}", reg_name(dst), reg_name(src)),
        X64Instruction::SetccRax { cc } => format!("set{} al; movzx rax, al", cc_name(cc)),
        X64Instruction::NegReg { dst } => format!("neg {}", reg_name(dst)),
        X64Instruction::CmpRegImm32(reg, imm) => format!("cmp {}, 0x{imm:X}", reg_name(reg)),
        X64Instruction::TestRegReg { dst, src } => format!("test {}, {}", reg_name(dst), reg_name(src)),
        X64Instruction::Je(label) => format!("je {label}"),
        X64Instruction::Jne(label) => format!("jne {label}"),
        X64Instruction::Jmp(label) => format!("jmp {label}"),
        X64Instruction::CallLabel(label) => format!("call {label}"),
        X64Instruction::CallReg(reg) => format!("call {}", reg_name(reg)),
        X64Instruction::Syscall => "syscall".to_string(),
        X64Instruction::Ret => "ret".to_string(),
    }
}

/// 返回 `Reg64` 的小写汇编名。
fn reg_name(reg: &Reg64) -> &'static str {
    match reg {
        Reg64::Rax => "rax",
        Reg64::Rcx => "rcx",
        Reg64::Rdx => "rdx",
        Reg64::Rbx => "rbx",
        Reg64::Rsp => "rsp",
        Reg64::Rbp => "rbp",
        Reg64::Rsi => "rsi",
        Reg64::Rdi => "rdi",
        Reg64::R8 => "r8",
        Reg64::R9 => "r9",
        Reg64::R10 => "r10",
        Reg64::R11 => "r11",
        Reg64::R12 => "r12",
        Reg64::R13 => "r13",
        Reg64::R14 => "r14",
        Reg64::R15 => "r15",
    }
}

/// 返回 `ConditionCode` 的助记符后缀（如 `e` / `ne` / `l` 等）。
fn cc_name(cc: &ConditionCode) -> &'static str {
    match cc {
        ConditionCode::Equal => "e",
        ConditionCode::NotEqual => "ne",
        ConditionCode::Less => "l",
        ConditionCode::LessEqual => "le",
        ConditionCode::Greater => "g",
        ConditionCode::GreaterEqual => "ge",
    }
}

/// 估算指令长度，用于在反汇编输出中标注偏移。
///
/// 该估算与 `encode.rs` 的实际编码保持一致，仅用于诊断显示；
/// 精确偏移以 `EncodedModule.labels` 为准。
fn estimate_instruction_size(ins: &X64Instruction) -> u32 {
    match ins {
        X64Instruction::Label { .. } => 0,
        X64Instruction::SubRsp(imm) => {
            if *imm <= 0x7F {
                4
            }
            else {
                7
            }
        }
        X64Instruction::AddRsp(imm) => {
            if *imm <= 0x7F {
                4
            }
            else {
                7
            }
        }
        X64Instruction::MovRegImm32(reg, _) => {
            if reg.needs_rex() {
                6
            }
            else {
                5
            }
        }
        X64Instruction::MovRegImm64(reg, _) => {
            if reg.needs_rex() {
                11
            }
            else {
                10
            }
        }
        X64Instruction::MovRegReg { dst, src } => 3 + rex_prefix_len(dst, src),
        X64Instruction::XorReg { dst, src } => 2 + rex_prefix_len(dst, src),
        X64Instruction::LeaRipRelative { dst, .. } => 7 + rex_prefix_len(dst, &Reg64::Rax),
        X64Instruction::CallImport { .. } => 6,
        X64Instruction::MovStackArgQword { .. } => 9,
        X64Instruction::LeaRspOffset { dst, .. } => 8 + rex_prefix_len(dst, &Reg64::Rax),
        X64Instruction::MovRegMemReg { dst, base, .. } => 3 + rex_prefix_len(dst, base),
        X64Instruction::MovRegRspOffset { dst, .. } => 8 + rex_prefix_len(dst, &Reg64::Rax),
        X64Instruction::MovRspOffsetReg32 { src, .. } => 8 + rex_prefix_len(src, &Reg64::Rax),
        X64Instruction::MovMemRegImm32 { base, .. } => 7 + rex_prefix_len(base, &Reg64::Rax),
        X64Instruction::MovRspOffsetImm32 { .. } => 9,
        X64Instruction::AddRegReg { dst, src } => 3 + rex_prefix_len(dst, src),
        X64Instruction::SubRegReg { dst, src } => 3 + rex_prefix_len(dst, src),
        X64Instruction::ImulRegReg { dst, src } => 4 + rex_prefix_len(dst, src),
        X64Instruction::IdivReg { divisor } => 4 + rex_prefix_len(divisor, &Reg64::Rax),
        X64Instruction::CmpRegReg { dst, src } => 3 + rex_prefix_len(dst, src),
        X64Instruction::SetccRax { .. } => 6,
        X64Instruction::NegReg { dst } => 3 + rex_prefix_len(dst, &Reg64::Rax),
        X64Instruction::CmpRegImm32(reg, _) => 6 + rex_prefix_len(reg, &Reg64::Rax),
        X64Instruction::TestRegReg { dst, src } => 3 + rex_prefix_len(dst, src),
        X64Instruction::Je { .. } | X64Instruction::Jne { .. } => 6,
        X64Instruction::Jmp { .. } => 5,
        X64Instruction::CallLabel { .. } => 5,
        X64Instruction::CallReg(reg) => 2 + rex_prefix_len(reg, &Reg64::Rax),
        X64Instruction::Syscall => 2,
        X64Instruction::Ret => 1,
    }
}

/// 计算 REX 前缀长度（0 或 1）。
fn rex_prefix_len(a: &Reg64, b: &Reg64) -> u32 {
    if a.is_extended() || b.is_extended() { 1 } else { 0 }
}
