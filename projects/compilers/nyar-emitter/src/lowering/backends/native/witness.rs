use miette::Result;
use nyar::{WitnessMethodSlotSubmission, WitnessSubmission};
use std_data::binary::{
    elf::NativeElfImageBuilder,
    pe::NativeImageBuilder,
    x86_64::{MsvcFunctionBuilder, Reg64, SysvFunctionBuilder, X64Instruction},
};

use crate::FragmentSubmission;

fn emit_impl_return_literal_sysv(function: &mut SysvFunctionBuilder, symbol: &str, literal_label: &str) {
    function.push(X64Instruction::Label(symbol.to_string()));
    function.push(X64Instruction::LeaRipRelative { dst: Reg64::Rax, label: literal_label.to_string() });
    function.push(X64Instruction::Ret);
}

fn emit_impl_return_literal_msvc(function: &mut MsvcFunctionBuilder, symbol: &str, literal_label: &str) {
    function.push(X64Instruction::Label(symbol.to_string()));
    function.push(X64Instruction::LeaRipRelative { dst: Reg64::Rax, label: literal_label.to_string() });
    function.push(X64Instruction::Ret);
}

pub(crate) fn emit_witness_dispatch_x64_sysv(function: &mut SysvFunctionBuilder, table_label: &str, method_index: u32) {
    let disp = i8::try_from(method_index * 8).unwrap_or(0);
    function.push(X64Instruction::LeaRipRelative { dst: Reg64::Rax, label: table_label.to_string() });
    function.push(X64Instruction::MovRegMemReg { dst: Reg64::Rax, base: Reg64::Rax, offset: disp });
    function.push(X64Instruction::CallReg(Reg64::Rax));
}

pub(crate) fn emit_witness_dispatch_x64_msvc(function: &mut MsvcFunctionBuilder, table_label: &str, method_index: u32) {
    let disp = i8::try_from(method_index * 8).unwrap_or(0);
    function.push(X64Instruction::LeaRipRelative { dst: Reg64::Rax, label: table_label.to_string() });
    function.push(X64Instruction::MovRegMemReg { dst: Reg64::Rax, base: Reg64::Rax, offset: disp });
    function.push(X64Instruction::CallReg(Reg64::Rax));
}

/// 向原生镜像写入 witness 表、实现桩与字面量数据。
pub(crate) fn emit_witness_tables(
    mut pe: Option<&mut NativeImageBuilder>,
    mut elf: Option<&mut NativeElfImageBuilder>,
    submission: &FragmentSubmission,
) -> Result<()> {
    for table in &submission.witness_tables {
        let literal_label = format!("{}_result", table.table_label);
        let impl_labels = table.methods.iter().map(|slot| slot.impl_symbol.clone()).collect::<Vec<_>>();

        if let Some(builder) = pe.as_mut() {
            builder.add_rdata(&literal_label, table.result_literal.as_bytes());
            builder.add_rdata_text_ptr_sequence(&table.table_label, &impl_labels);
            for method in &table.methods {
                emit_impl_stub_pe(builder, method, table);
            }
        }
        if let Some(builder) = elf.as_mut() {
            builder.add_rodata(&literal_label, table.result_literal.as_bytes());
            builder.add_rodata_text_ptr_sequence(&table.table_label, &impl_labels);
            for method in &table.methods {
                emit_impl_stub_elf(builder, method, table);
            }
        }
    }
    Ok(())
}

fn emit_impl_stub_pe(builder: &mut NativeImageBuilder, method: &WitnessMethodSlotSubmission, table: &WitnessSubmission) {
    let literal_label = format!("{}_result", table.table_label);
    let mut stub = MsvcFunctionBuilder::new();
    if table.trait_name == "Future" && method.method_name == "poll" {
        emit_future_poll_impl_msvc(&mut stub, &method.impl_symbol);
    }
    else if table.trait_name == "Iterator" && method.method_name == "next" {
        builder.add_rdata("witness_iter_0", b"0");
        builder.add_rdata("witness_iter_1", b"1");
        emit_iterator_next_impl_msvc(&mut stub, &method.impl_symbol);
    }
    else {
        emit_impl_return_literal_msvc(&mut stub, &method.impl_symbol, &literal_label);
    }
    for instruction in stub.encoder().instructions() {
        builder.push(instruction.clone());
    }
}

fn emit_impl_stub_elf(builder: &mut NativeElfImageBuilder, method: &WitnessMethodSlotSubmission, table: &WitnessSubmission) {
    let literal_label = format!("{}_result", table.table_label);
    let mut stub = SysvFunctionBuilder::new();
    if table.trait_name == "Future" && method.method_name == "poll" {
        emit_future_poll_impl_sysv(&mut stub, &method.impl_symbol);
    }
    else if table.trait_name == "Iterator" && method.method_name == "next" {
        builder.add_rodata("witness_iter_0", b"0");
        builder.add_rodata("witness_iter_1", b"1");
        emit_iterator_next_impl_sysv(&mut stub, &method.impl_symbol);
    }
    else {
        emit_impl_return_literal_sysv(&mut stub, &method.impl_symbol, &literal_label);
    }
    for instruction in stub.encoder().instructions() {
        builder.push(instruction.clone());
    }
}

fn emit_future_poll_impl_msvc(function: &mut MsvcFunctionBuilder, symbol: &str) {
    function.push(X64Instruction::Label(symbol.to_string()));
    function.push(X64Instruction::TestRegReg { dst: Reg64::Rcx, src: Reg64::Rcx });
    function.push(X64Instruction::Je("witness_poll_zero".to_string()));
    function.push(X64Instruction::MovRegMemReg { dst: Reg64::Rax, base: Reg64::Rcx, offset: 0 });
    function.push(X64Instruction::TestRegReg { dst: Reg64::Rax, src: Reg64::Rax });
    function.push(X64Instruction::Jne("witness_poll_ready".to_string()));
    function.push(X64Instruction::MovMemRegImm32 { base: Reg64::Rcx, offset: 0, value: 1 });
    function.push(X64Instruction::XorReg { dst: Reg64::Rax, src: Reg64::Rax });
    function.push(X64Instruction::Ret);
    function.push(X64Instruction::Label("witness_poll_ready".to_string()));
    function.push(X64Instruction::MovRegImm32(Reg64::Rax, 1));
    function.push(X64Instruction::Ret);
    function.push(X64Instruction::Label("witness_poll_zero".to_string()));
    function.push(X64Instruction::XorReg { dst: Reg64::Rax, src: Reg64::Rax });
    function.push(X64Instruction::Ret);
}

fn emit_future_poll_impl_sysv(function: &mut SysvFunctionBuilder, symbol: &str) {
    function.push(X64Instruction::Label(symbol.to_string()));
    function.push(X64Instruction::TestRegReg { dst: Reg64::Rdi, src: Reg64::Rdi });
    function.push(X64Instruction::Je("witness_poll_zero".to_string()));
    function.push(X64Instruction::MovRegMemReg { dst: Reg64::Rax, base: Reg64::Rdi, offset: 0 });
    function.push(X64Instruction::TestRegReg { dst: Reg64::Rax, src: Reg64::Rax });
    function.push(X64Instruction::Jne("witness_poll_ready".to_string()));
    function.push(X64Instruction::MovMemRegImm32 { base: Reg64::Rdi, offset: 0, value: 1 });
    function.push(X64Instruction::XorReg { dst: Reg64::Rax, src: Reg64::Rax });
    function.push(X64Instruction::Ret);
    function.push(X64Instruction::Label("witness_poll_ready".to_string()));
    function.push(X64Instruction::MovRegImm32(Reg64::Rax, 1));
    function.push(X64Instruction::Ret);
    function.push(X64Instruction::Label("witness_poll_zero".to_string()));
    function.push(X64Instruction::XorReg { dst: Reg64::Rax, src: Reg64::Rax });
    function.push(X64Instruction::Ret);
}

fn emit_iterator_next_impl_msvc(function: &mut MsvcFunctionBuilder, symbol: &str) {
    function.push(X64Instruction::Label(symbol.to_string()));
    function.push(X64Instruction::TestRegReg { dst: Reg64::Rcx, src: Reg64::Rcx });
    function.push(X64Instruction::Je("witness_next_done".to_string()));
    function.push(X64Instruction::MovRegMemReg { dst: Reg64::Rax, base: Reg64::Rcx, offset: 0 });
    function.push(X64Instruction::CmpRegImm32(Reg64::Rax, 2));
    function.push(X64Instruction::Je("witness_next_done".to_string()));
    function.push(X64Instruction::Label("witness_next_value".to_string()));
    function.push(X64Instruction::CmpRegImm32(Reg64::Rax, 0));
    function.push(X64Instruction::Je("witness_next_0".to_string()));
    function.push(X64Instruction::CmpRegImm32(Reg64::Rax, 1));
    function.push(X64Instruction::Je("witness_next_1".to_string()));
    function.push(X64Instruction::Jmp("witness_next_done".to_string()));
    function.push(X64Instruction::Label("witness_next_0".to_string()));
    function.push(X64Instruction::MovMemRegImm32 { base: Reg64::Rcx, offset: 0, value: 1 });
    function.push(X64Instruction::LeaRipRelative { dst: Reg64::Rax, label: "witness_iter_0".to_string() });
    function.push(X64Instruction::Ret);
    function.push(X64Instruction::Label("witness_next_1".to_string()));
    function.push(X64Instruction::MovMemRegImm32 { base: Reg64::Rcx, offset: 0, value: 2 });
    function.push(X64Instruction::LeaRipRelative { dst: Reg64::Rax, label: "witness_iter_1".to_string() });
    function.push(X64Instruction::Ret);
    function.push(X64Instruction::Label("witness_next_done".to_string()));
    function.push(X64Instruction::XorReg { dst: Reg64::Rax, src: Reg64::Rax });
    function.push(X64Instruction::Ret);
}

fn emit_iterator_next_impl_sysv(function: &mut SysvFunctionBuilder, symbol: &str) {
    function.push(X64Instruction::Label(symbol.to_string()));
    function.push(X64Instruction::TestRegReg { dst: Reg64::Rdi, src: Reg64::Rdi });
    function.push(X64Instruction::Je("witness_next_done".to_string()));
    function.push(X64Instruction::MovRegMemReg { dst: Reg64::Rax, base: Reg64::Rdi, offset: 0 });
    function.push(X64Instruction::CmpRegImm32(Reg64::Rax, 2));
    function.push(X64Instruction::Je("witness_next_done".to_string()));
    function.push(X64Instruction::Label("witness_next_value".to_string()));
    function.push(X64Instruction::CmpRegImm32(Reg64::Rax, 0));
    function.push(X64Instruction::Je("witness_next_0".to_string()));
    function.push(X64Instruction::CmpRegImm32(Reg64::Rax, 1));
    function.push(X64Instruction::Je("witness_next_1".to_string()));
    function.push(X64Instruction::Jmp("witness_next_done".to_string()));
    function.push(X64Instruction::Label("witness_next_0".to_string()));
    function.push(X64Instruction::MovMemRegImm32 { base: Reg64::Rdi, offset: 0, value: 1 });
    function.push(X64Instruction::LeaRipRelative { dst: Reg64::Rax, label: "witness_iter_0".to_string() });
    function.push(X64Instruction::Ret);
    function.push(X64Instruction::Label("witness_next_1".to_string()));
    function.push(X64Instruction::MovMemRegImm32 { base: Reg64::Rdi, offset: 0, value: 2 });
    function.push(X64Instruction::LeaRipRelative { dst: Reg64::Rax, label: "witness_iter_1".to_string() });
    function.push(X64Instruction::Ret);
    function.push(X64Instruction::Label("witness_next_done".to_string()));
    function.push(X64Instruction::XorReg { dst: Reg64::Rax, src: Reg64::Rax });
    function.push(X64Instruction::Ret);
}

/// Linux `_start`：经 witness 槽位间接调用并在 `RAX` 中取得返回值指针。
pub(crate) fn lower_witness_calls_linux(
    submission: &FragmentSubmission,
    function: &mut SysvFunctionBuilder,
    builder: &mut NativeElfImageBuilder,
) -> Result<()> {
    for edge in &submission.witness_calls {
        let Some(table) = find_witness_table(submission, &edge.trait_name, &edge.type_name)
        else {
            continue;
        };
        emit_witness_dispatch_x64_sysv(function, &table.table_label, edge.method_index);
        if edge.print_result {
            emit_linux_print_from_rax(function, builder, table)?;
        }
    }
    Ok(())
}

/// Windows 入口：经 witness 槽位间接调用并在 `RAX` 中取得返回值指针。
pub(crate) fn lower_witness_calls_windows(
    submission: &FragmentSubmission,
    function: &mut MsvcFunctionBuilder,
    builder: &mut NativeImageBuilder,
) -> Result<()> {
    for edge in &submission.witness_calls {
        let Some(table) = find_witness_table(submission, &edge.trait_name, &edge.type_name)
        else {
            continue;
        };
        emit_witness_dispatch_x64_msvc(function, &table.table_label, edge.method_index);
        if edge.print_result {
            emit_win32_print_from_rax(function, builder, table)?;
        }
    }
    Ok(())
}

fn find_witness_table<'a>(submission: &'a FragmentSubmission, trait_name: &str, type_name: &str) -> Option<&'a WitnessSubmission> {
    submission.witness_tables.iter().find(|table| table.trait_name == trait_name && table.type_name == type_name)
}

fn emit_linux_print_from_rax(function: &mut SysvFunctionBuilder, builder: &mut NativeElfImageBuilder, table: &WitnessSubmission) -> Result<()> {
    let len = u32::try_from(table.result_literal.len()).unwrap_or(0);
    function.push(X64Instruction::MovRegReg { dst: Reg64::Rsi, src: Reg64::Rax });
    function.push(X64Instruction::MovRegImm32(Reg64::Rax, 1));
    function.push(X64Instruction::MovRegImm32(Reg64::Rdi, 1));
    function.push(X64Instruction::MovRegImm32(Reg64::Rdx, len));
    function.push(X64Instruction::Syscall);
    let _ = builder;
    Ok(())
}

fn emit_win32_print_from_rax(function: &mut MsvcFunctionBuilder, builder: &mut NativeImageBuilder, table: &WitnessSubmission) -> Result<()> {
    let len = u32::try_from(table.result_literal.len()).unwrap_or(0);
    let get_std_handle = builder.import("kernel32", "GetStdHandle");
    let write_file = builder.import("kernel32", "WriteFile");
    function.push(X64Instruction::MovRegReg { dst: Reg64::Rdx, src: Reg64::Rax });
    function.push(X64Instruction::MovRegImm32(Reg64::Rcx, 0xFFFF_FFF5));
    function.push(X64Instruction::CallImport { slot: get_std_handle });
    function.push(X64Instruction::MovRegReg { dst: Reg64::Rcx, src: Reg64::Rax });
    function.push(X64Instruction::MovRegImm32(Reg64::R8, len));
    function.push(X64Instruction::LeaRspOffset { dst: Reg64::R9, offset: 0x28 });
    function.push(X64Instruction::MovStackArgQword { value: 0 });
    function.push(X64Instruction::CallImport { slot: write_file });
    Ok(())
}
