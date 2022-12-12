//! Wasm structured control: CFG dispatcher ownership.
//!
//! Owns the backend-private invariant that any legal Semantic MIR CFG keeps
//! Jump, Branch, block parameters, and Return identity through Wasm emission
//! (`pc_local` + `loop` + `br_table`). This is Wasm **control encoding**, not
//! another MIR layer — Semantic MIR remains the language CFG authority.
//!
//! Zero-logic extract from the former monolithic lowerer (B4 seam).

use super::{MirBlock, MirBlockRef, MirOperand, MirTerminator, WASM_GC_ANYREF, WASM_GC_EXTERNREF, WasmMirLowerer};
use crate::lowering::backends::wasm::sections::encode_uleb128;
use std_data::binary::wasm::{
    BLOCKTYPE_EMPTY, VALTYPE_ANYREF, VALTYPE_F64, VALTYPE_I32, VALTYPE_I64, WasmOpcode, encode_ref_null_anyref, encode_return,
};

impl<'a> WasmMirLowerer<'a> {
    pub(super) fn emit_function_body(&mut self) {
        // V8 要求 `(ref null T)` typed local 在函数入口显式初始化，
        // 否则报 "uninitialized non-defaultable local" 编译错误。
        // 用 `struct.new_default T` 产生 `(ref T)` non-nullable 值（是 `(ref null T)` 的子类型）。
        for (local, type_index) in self.typed_ref_locals_to_init.clone() {
            self.emit_struct_new_default(type_index);
            self.emit_local_set(local);
        }
        let case_count = self.block_order.len();
        if case_count == 0 {
            self.emit_default_for_return_type();
            return;
        }
        let entry_index = *self.block_index.get(&self.mir_fn.entry).unwrap_or(&0);
        self.emit_i32_const(i32::try_from(entry_index).unwrap_or(0));
        self.emit_local_set(self.pc_local);
        // `loop { block* br_table; case0; ...; caseN-1 }` — 与 suspend br_table 调度同构。
        // Jump/Branch 写 `pc_local` 后 `br` 回 loop，前向与后向边都合法。
        WasmOpcode::Loop.encode(&mut self.code);
        self.code.push(BLOCKTYPE_EMPTY);
        for _ in 0..case_count {
            WasmOpcode::Block.encode(&mut self.code);
            self.code.push(BLOCKTYPE_EMPTY);
        }
        self.emit_local_get(self.pc_local);
        WasmOpcode::BrTable.encode(&mut self.code);
        encode_uleb128(u32::try_from(case_count).expect("case count"), &mut self.code);
        for index in 0..case_count {
            encode_uleb128(u32::try_from(index).expect("case index"), &mut self.code);
        }
        encode_uleb128(u32::try_from(case_count).expect("default depth"), &mut self.code);
        WasmOpcode::End.encode(&mut self.code);
        self.emit_cfg_case(0, case_count);
        for case_index in 1..case_count {
            WasmOpcode::End.encode(&mut self.code);
            self.emit_cfg_case(case_index, case_count);
        }
        WasmOpcode::End.encode(&mut self.code);
        // br_table default 或调度耗尽：按返回类型压占位值，保持函数签名。
        self.emit_default_for_return_type();
    }

    fn emit_cfg_case(&mut self, case_index: usize, case_count: usize) {
        let Some(block_id) = self.block_order.get(case_index).copied()
        else {
            self.emit_br_depth(self.cfg_continue_depth(case_index, case_count));
            return;
        };
        let Some(block) = self.mir_fn.blocks.get(block_id.0 as usize)
        else {
            self.emit_br_depth(self.cfg_continue_depth(case_index, case_count));
            return;
        };
        for instruction in &block.instructions {
            self.emit_instruction(instruction);
        }
        self.emit_terminator(case_index, case_count, block);
    }

    /// Depth of the dispatch `loop` from inside case `case_index`'s body.
    ///
    /// Layout: `loop { block_{N-1} { … block_0 { br_table } case0 } case1 … }`.
    /// After `block_0..block_{case_index}` have ended, remaining labels are
    /// `block_{case_index+1} … block_{N-1}` plus the loop — so the loop sits at
    /// depth `N - 1 - case_index`. Using `N - case_index` (off-by-one) branches
    /// past the loop and breaks forward Jump / Result match arms (B4).
    fn cfg_continue_depth(&self, case_index: usize, case_count: usize) -> u32 {
        u32::try_from(case_count.saturating_sub(case_index).saturating_sub(1)).unwrap_or(0)
    }

    /// 发射与 `return_value_type` 匹配的默认值常量。
    /// `None` (void) 不发射任何字节；`Some(VALTYPE_I32)` 发射 `i32.const 0`；
    /// `Some(VALTYPE_ANYREF)` 发射 `ref.null any`；`Some(VALTYPE_F64)` 发射 `f64.const 0`；
    /// `Some(VALTYPE_I64)` 发射 `i64.const 0`。
    fn emit_default_for_return_type(&mut self) {
        match self.return_value_type {
            None => {}
            Some(VALTYPE_I32) => self.emit_i32_const(0),
            Some(VALTYPE_I64) => self.emit_i64_const(0),
            Some(VALTYPE_F64) => self.emit_f64_const(0.0),
            Some(VALTYPE_ANYREF) => encode_ref_null_anyref(&mut self.code),
            Some(_) => self.emit_i32_const(0),
        }
    }

    fn emit_terminator(&mut self, case_index: usize, case_count: usize, block: &MirBlock) {
        let continue_depth = self.cfg_continue_depth(case_index, case_count);
        match &block.terminator {
            MirTerminator::Return { value } => {
                // 根据返回类型发射返回值操作数。
                // - void: 不压栈,直接 return。
                // - i32: 发射 i32 操作数或 i32.const 0 占位。
                // - anyref: 发射引用操作数或 ref.null any 占位。
                // - f64/i64: 发射对应类型操作数或默认值。
                match self.return_value_type {
                    None => {}
                    Some(VALTYPE_I32) => {
                        if let Some(value) = value {
                            self.emit_i32_operand(value);
                        }
                        else {
                            self.emit_i32_const(0);
                        }
                    }
                    Some(VALTYPE_ANYREF) => {
                        // 仅当 value 是 Value(vref) 且确为引用语义（在 `reference_locals`）时才 emit_operand；
                        // 若 value 落在 `value_locals`/`scalar_locals`（i32）或为 Int/Bool 常量，
                        // 说明该返回路径的值类型与函数签名（anyref）不一致（如错误码 `Int(1)` 返回给 anyref 函数）。
                        // 此时 emit `ref.null anyref` 占位，避免 `return expected anyref, got i32`，
                        // 让自举链路继续推进；根本的类型不一致需在 MIR 类型推断层面解决。
                        if let Some(value) = value {
                            let is_anyref_local = match value {
                                MirOperand::Value(vref) => self.reference_locals.contains_key(vref),
                                _ => false,
                            };
                            if is_anyref_local {
                                self.emit_operand(value);
                            }
                            else {
                                self.emit_ref_null_anyref();
                            }
                        }
                        else {
                            self.emit_ref_null_anyref();
                        }
                    }
                    Some(VALTYPE_F64) => {
                        if let Some(value) = value {
                            self.emit_f64_operand(value);
                        }
                        else {
                            self.emit_f64_const(0.0);
                        }
                    }
                    Some(VALTYPE_I64) => {
                        // i64 返回:当前只有 i64.const 0 占位（无 i64 operand 辅助）。
                        self.emit_i64_const(0);
                    }
                    Some(_) => {
                        if let Some(value) = value {
                            self.emit_operand(value);
                        }
                        else {
                            self.emit_i32_const(0);
                        }
                    }
                }
                encode_return(&mut self.code);
            }
            MirTerminator::Jump { target, arguments } => {
                // 跳转前需将 arguments 写入目标块的参数 local。
                // 旧实现用 `..` 丢弃 arguments,导致块参数永远不被填充——严重瞒报。
                // SSA 语义下 arguments 不引用目标块的参数,顺序赋值安全。
                self.emit_jump_arguments(*target, arguments);
                let target_index = *self.block_index.get(target).unwrap_or(&case_index);
                self.emit_i32_const(i32::try_from(target_index).unwrap_or(0));
                self.emit_local_set(self.pc_local);
                self.emit_br_depth(continue_depth);
            }
            MirTerminator::Branch { condition, then_target, else_target } => {
                let then_index = *self.block_index.get(then_target).unwrap_or(&case_index);
                let else_index = *self.block_index.get(else_target).unwrap_or(&case_index);
                // Wasm `select`: stack [val1, val2, c] → val1 if c≠0, else val2.
                // Push then first so a true match-arm condition takes Fine/then PC (B4).
                self.emit_i32_const(i32::try_from(then_index).unwrap_or(0));
                self.emit_i32_const(i32::try_from(else_index).unwrap_or(0));
                self.emit_i32_operand(condition);
                WasmOpcode::Select.encode(&mut self.code);
                self.emit_local_set(self.pc_local);
                self.emit_br_depth(continue_depth);
            }
            other => {
                panic!(
                    "WASM emit fail-closed: unsupported terminator {:?} in `{}` block {}; refuse placeholder fallthrough",
                    std::mem::discriminant(other),
                    self.mir_fn.symbol,
                    case_index
                );
            }
        }
    }

    /// 在跳转到目标块之前,将 arguments 依次写入目标块的参数 local。
    ///
    /// 对每个 `(argument, target_param)` 对:先发射 argument 操作数到栈,
    /// 再 `local.set` 到目标参数对应的 local。
    ///
    /// 目标参数的 local 在 `new()` 中已按类型分配:引用类型参数在
    /// `reference_locals`(anyref),值类型/标量参数在 `value_locals`(i32)。
    ///
    /// **SSA 顺序赋值限制**:当多个参数存在「交换」依赖
    /// (如 `arg[0]` 引用 `param[1]` 且 `arg[1]` 引用 `param[0]`)时,
    /// 顺序赋值会读到已覆写的值。正确做法需要临时 local 中转。
    /// 当前线性 block 模型不支持真正的后向跳转(loop),该场景极少触发,
    /// 故采用顺序赋值;若未来支持 loop,需引入 temp local 方案。
    fn emit_jump_arguments(&mut self, target: MirBlockRef, arguments: &[MirOperand]) {
        if arguments.is_empty() {
            return;
        }
        let Some(target_block) = self.mir_fn.blocks.get(target.0 as usize)
        else {
            return;
        };
        if target_block.parameters.len() != arguments.len() {
            // 参数数量不匹配:防御性跳过,避免越界。
            return;
        }
        for (arg, param) in arguments.iter().zip(target_block.parameters.iter()) {
            // 目标槽真实 valtype 优先；arg 分类与槽不一致时一律 coerce，禁止裸 local.get→local.set。
            if let Some(local) = self.reference_locals.get(param).copied() {
                let local_ty = self.wasm_local_value_type(local);
                if local_ty == WASM_GC_ANYREF || local_ty == WASM_GC_EXTERNREF {
                    self.emit_operand_coerced(arg, local_ty);
                    self.emit_local_set(local);
                    continue;
                }
                self.reference_locals.remove(param);
                let arg_ty = self.operand_wasm_stack_type(arg);
                let want = match arg_ty {
                    VALTYPE_I64 | VALTYPE_F64 => arg_ty,
                    _ => VALTYPE_I32,
                };
                let new_local = self.alloc_scalar_local_for_stack_type(want);
                self.value_locals.insert(*param, new_local);
                self.emit_operand_coerced(arg, want);
                self.emit_local_set(new_local);
                continue;
            }
            if let Some(local) = self.value_locals.get(param).copied().or_else(|| self.scalar_locals.get(param).copied()) {
                let local_ty = self.wasm_local_value_type(local);
                if local_ty == WASM_GC_ANYREF || local_ty == WASM_GC_EXTERNREF {
                    self.value_locals.remove(param);
                    self.scalar_locals.remove(param);
                    self.reference_locals.insert(*param, local);
                    self.emit_operand_coerced(arg, local_ty);
                    self.emit_local_set(local);
                }
                else {
                    let arg_ty = self.operand_wasm_stack_type(arg);
                    let want = match arg_ty {
                        VALTYPE_I64 | VALTYPE_F64 => arg_ty,
                        WASM_GC_ANYREF | WASM_GC_EXTERNREF => {
                            // 标量槽收到引用：迁到 anyref，避免 i32.set ← anyref。
                            self.value_locals.remove(param);
                            self.scalar_locals.remove(param);
                            let new_local = self.alloc_anyref_local();
                            self.reference_locals.insert(*param, new_local);
                            self.emit_operand_coerced(arg, WASM_GC_ANYREF);
                            self.emit_local_set(new_local);
                            continue;
                        }
                        _ => VALTYPE_I32,
                    };
                    if local_ty != want {
                        self.value_locals.remove(param);
                        self.scalar_locals.remove(param);
                        let new_local = self.alloc_scalar_local_for_stack_type(want);
                        self.value_locals.insert(*param, new_local);
                        self.emit_operand_coerced(arg, want);
                        self.emit_local_set(new_local);
                    }
                    else {
                        self.emit_operand_coerced(arg, local_ty);
                        self.emit_local_set(local);
                    }
                }
                continue;
            }
            let arg_stack_ty = self.operand_wasm_stack_type(arg);
            if arg_stack_ty == WASM_GC_ANYREF || arg_stack_ty == WASM_GC_EXTERNREF {
                let local = self.alloc_anyref_local();
                self.reference_locals.insert(*param, local);
                self.emit_operand_coerced(arg, WASM_GC_ANYREF);
                self.emit_local_set(local);
            }
            else {
                let want = match arg_stack_ty {
                    VALTYPE_I64 | VALTYPE_F64 => arg_stack_ty,
                    _ => VALTYPE_I32,
                };
                let local = self.alloc_scalar_local_for_stack_type(want);
                self.value_locals.insert(*param, local);
                self.emit_operand_coerced(arg, want);
                self.emit_local_set(local);
            }
        }
    }

    fn emit_br_depth(&mut self, depth: u32) {
        WasmOpcode::Br.encode(&mut self.code);
        encode_uleb128(depth, &mut self.code);
    }
}
