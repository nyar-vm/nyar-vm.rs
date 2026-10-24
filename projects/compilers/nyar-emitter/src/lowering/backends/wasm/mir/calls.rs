//! Resolved Semantic MIR calls -> Wasm call forms.
//!
//! Owns direct / import / intrinsic / witness / indirect call emission and the
//! call-shaped helpers currently on the same dispatch path. This is Wasm
//! **call encoding**, not a second MIR. Semantic MIR remains call-resolution
//! authority; this module only maps already-resolved calls to Wasm opcodes.

use super::{numeric, *};
use crate::target_instruction::{TargetInstructionEncoding, TargetOperandRecipe, TargetOperandType};

/// 计划冻结的目标标量载体 → wasm 栈类型字节。
fn wasm_stack_type(operand_type: TargetOperandType) -> u8 {
    match operand_type {
        TargetOperandType::I32 => VALTYPE_I32,
        TargetOperandType::I64 => VALTYPE_I64,
        TargetOperandType::F64 => VALTYPE_F64,
    }
}

impl<'a> WasmMirLowerer<'a> {
    /// Lowers a non-builtin MIR call to WASM bytecode.
    ///
    /// - Static dispatch emits `call` (0x10) with the resolved function index.
    /// - Witness dispatch emits `call_indirect` (0x11) using the witness operand as the table index.
    /// - `receiver_kind: Some(ByAddress)` means the first argument is a value-type receiver
    ///   whose linear-memory address is already an `i32` local in `value_locals`.
    ///   Because `emit_operand` emits `local.get` for value locals, the receiver's
    ///   linear-memory address is passed directly as the first WASM parameter -?no
    ///   special codegen is needed beyond emitting the argument normally.
    pub(super) fn emit_call_lowering(
        &mut self,
        callee: &MirOperand,
        arguments: &[MirOperand],
        dispatch: MirDispatchKind,
        witness: Option<&MirOperand>,
        receiver_kind: Option<ReceiverPassingKind>,
        output: Option<MirValueRef>,
    ) -> miette::Result<()> {
        if let Some(ReceiverPassingKind::ByAddress) = receiver_kind {
            // Confirm the receiver exists as the first argument; it is emitted below.
        }
        let callee_return;
        match dispatch {
            MirDispatchKind::Static => {
                // `[wasm_opcode]` 绑定的 provider 直接在调用点编码为目标指令；
                // 不是 host import，也没有函数体，必须先于 import 解析处理。
                if self.try_emit_wasm_opcode_call(callee, arguments, output)? {
                    return Ok(());
                }
                if let Some(import_index) = self.resolve_callee_import_index(callee, arguments) {
                    if self.try_emit_wasi_cli_write_via_stream(arguments, import_index, output) {
                        return Ok(());
                    }
                    callee_return = self.resolve_callee_return_type(callee, Some(import_index));
                    let param_types = self.resolve_callee_param_types(callee, Some(import_index));
                    self.emit_call_arguments(arguments, &param_types);
                    self.emit_call(import_index);
                }
                else if let Some(function_index) = self.resolve_callee_function_index(callee) {
                    callee_return = *self.return_types_by_function_index.get(&function_index).expect("WASM 函数下标缺少返回签名");
                    let param_types = self.param_types_by_function_index.get(&function_index).expect("WASM 函数下标缺少参数签名").clone();
                    self.emit_call_arguments(arguments, &param_types);
                    self.emit_call(function_index);
                }
                else {
                    panic!("WASM 静态调用缺少已解析 callable identity: {} callee={callee:?}", self.mir_fn.symbol);
                }
            }
            MirDispatchKind::Witness => {
                callee_return = self.resolve_callee_return_type(callee, None);
                for argument in arguments {
                    self.emit_operand_coerced(argument, self.operand_wasm_stack_type(argument));
                }
                if let Some(witness_operand) = witness {
                    self.emit_operand(witness_operand);
                }
                else {
                    panic!("WASM witness 调用缺少显式分派操作数: {}", self.mir_fn.symbol);
                }
                let type_index = self.resolve_callee_type_index(callee);
                self.emit_call_indirect(type_index, 0);
            }
            MirDispatchKind::Indirect => {
                panic!("WASM 缺少类型化间接调用计划: {}", self.mir_fn.symbol);
            }
        }
        self.emit_store_call_output(output, callee_return);
        Ok(())
    }

    fn emit_store_call_output(&mut self, output: Option<MirValueRef>, callee_return: Option<u8>) {
        match output {
            Some(output) => {
                let return_type = callee_return.expect("WASM 有 SSA 结果的调用缺少返回合同");
                self.validate_output_slot_type(output, return_type);
                self.assign_output_local(output);
            }
            None if callee_return.is_some() => WasmOpcode::Drop.encode(&mut self.code),
            None => {}
        }
    }

    /// 编码阶段只验证预规划槽，不改变 SSA 表示。
    pub(super) fn validate_output_slot_type(&self, output: MirValueRef, stack_ty: u8) {
        let local = self.planned_value_local(output);
        assert_eq!(self.wasm_local_value_type(local), stack_ty, "WASM 编码结果与预规划槽冲突: %{} in {}", output.0, self.mir_fn.symbol);
    }

    pub(super) fn assign_output_local(&mut self, output: MirValueRef) {
        let local = self.planned_value_local(output);
        self.emit_local_set(local);
    }

    pub(super) fn resolve_callee_param_types(&self, callee: &MirOperand, import_index: Option<u32>) -> Vec<u8> {
        if let Some(index) = import_index {
            return self.import_param_types.get(index as usize).expect("WASM 导入缺少参数签名").clone();
        }
        let MirOperand::Item(instance) = callee
        else {
            panic!("WASM 普通调用缺少实例身份");
        };
        self.param_types_by_instance.get(instance).expect("WASM 实例缺少参数签名").clone()
    }

    pub(super) fn resolve_callee_return_type(&self, callee: &MirOperand, import_index: Option<u32>) -> Option<u8> {
        if let Some(index) = import_index {
            return *self.import_return_types.get(index as usize).expect("WASM 导入缺少返回签名");
        }
        let MirOperand::Item(instance) = callee
        else {
            panic!("WASM 普通调用缺少实例身份");
        };
        *self.return_types_by_instance.get(instance).expect("WASM 实例缺少返回签名")
    }

    pub(super) fn operand_wasm_stack_type(&self, operand: &MirOperand) -> u8 {
        match operand {
            MirOperand::Value(value) => self.wasm_local_value_type(self.planned_value_local(*value)),
            MirOperand::Constant(constant) => match constant {
                MirConstant::Utf8(_) => VALTYPE_I32,
                MirConstant::Utf16(_) => {
                    panic!("WASM lowering requires an explicit UTF-16 ABI contract")
                }
                MirConstant::Unit => WASM_GC_ANYREF,
                MirConstant::Float64(_) => VALTYPE_F64,
                MirConstant::Int(_) | MirConstant::Bool(_) => VALTYPE_I32,
            },
            MirOperand::Symbol(_) => panic!("WASM 值操作数缺少 SSA 身份"),
            MirOperand::Item(_) => {
                panic!("WASM callable identity cannot be used as a value operand")
            }
        }
    }

    pub(super) fn emit_operand_coerced(&mut self, operand: &MirOperand, expected: u8) {
        let actual = self.operand_wasm_stack_type(operand);
        if actual == expected {
            self.emit_operand(operand);
            return;
        }
        // Reference-local metadata is authoritative for GC aggregates：槽位已是引用，
        // 即便表面分类成 i32 也不得用 ref.null 顶替。
        if expected == WASM_GC_ANYREF && self.operand_reference_local(operand).is_some() {
            self.emit_operand(operand);
            return;
        }
        // 仅允许 TargetExecutionPlan 已承诺的显式宽度转换；禁止用 0 / null 掩盖 carrier 失配。
        if expected == VALTYPE_I64 && (actual == WASM_GC_ANYREF || actual == WASM_GC_EXTERNREF) {
            self.emit_operand(operand);
            self.emit_unbox_i64_payload();
            return;
        }
        if expected == VALTYPE_I64 && actual == VALTYPE_I32 {
            self.emit_operand(operand);
            WasmOpcode::I64ExtendI32S.encode(&mut self.code);
            return;
        }
        if expected == VALTYPE_I32 && actual == VALTYPE_I64 {
            self.emit_operand(operand);
            WasmOpcode::I32WrapI64.encode(&mut self.code);
            return;
        }
        if expected == WASM_GC_ANYREF && actual == VALTYPE_I64 {
            self.emit_box_i64_payload(operand);
            return;
        }
        if expected == WASM_GC_EXTERNREF && actual == WASM_GC_ANYREF {
            self.emit_operand(operand);
            return;
        }
        panic!(
            "WASM ABI carrier mismatch in `{}`: expected={expected:#x} actual={actual:#x}; \
             TargetExecutionPlan must provide an explicit conversion — refusing null/0 placeholders",
            self.mir_fn.symbol
        );
    }

    /// `[wasm_opcode("...")]` 绑定：把契约调用直接编码为目标数值指令。
    ///
    /// 只有 callee 携带已绑定 host contract 的实例身份时才命中；未绑定 provider
    /// 的 host contract 与普通函数调用都返回 `false`，走既有 import / local call
    /// 路径。arity、操作数配方、结果载体与 opcode 全部来自计划冻结的
    /// `TargetInstruction`；未冻结的绑定必须返回结构化错误，禁止从助记符猜语义。
    pub(super) fn try_emit_wasm_opcode_call(
        &mut self,
        callee: &MirOperand,
        arguments: &[MirOperand],
        output: Option<MirValueRef>,
    ) -> miette::Result<bool> {
        let MirOperand::Item(instance) = callee
        else {
            return Ok(false);
        };
        let Some((mnemonic, binding)) = self.callee_wasm_opcode.get(instance)
        else {
            return Ok(false);
        };
        let TargetOpcodeBinding::Frozen(instruction) = binding
        else {
            return Err(miette!("WASM `wasm_opcode` 绑定缺少可编码助记符 `{mnemonic}`: callee={instance:?} function={}", self.mir_fn.symbol));
        };
        let TargetInstructionEncoding::Wasm(opcode) = instruction.encoding
        else {
            return Err(miette!(
                "WASM `wasm_opcode` 绑定 `{mnemonic}` 冻结了非 wasm 编码: callee={instance:?} function={}",
                self.mir_fn.symbol
            ));
        };
        if arguments.len() != instruction.arity {
            return Err(miette!(
                "WASM `wasm_opcode` 绑定 `{mnemonic}` 实参数量与冻结 arity 不符: callee={instance:?} function={} expected={} actual={}",
                self.mir_fn.symbol,
                instruction.arity,
                arguments.len()
            ));
        }
        let operand_type = wasm_stack_type(instruction.operand_type);
        match instruction.recipe {
            TargetOperandRecipe::Operands => {
                self.emit_wasm_opcode_operands(mnemonic, operand_type, arguments);
                numeric::emit_opcode(opcode, &mut self.code);
            }
            TargetOperandRecipe::IntegerNegate => {
                match instruction.operand_type {
                    TargetOperandType::I32 => self.emit_i32_const(0),
                    TargetOperandType::I64 => self.emit_i64_const(0),
                    TargetOperandType::F64 => {
                        return Err(miette!(
                            "WASM `wasm_opcode` 绑定 `{mnemonic}` 冻结了整数取负配方但载体不是整数: callee={instance:?} function={}",
                            self.mir_fn.symbol
                        ));
                    }
                }
                self.emit_wasm_opcode_operands(mnemonic, operand_type, arguments);
                numeric::emit_opcode(opcode, &mut self.code);
            }
            TargetOperandRecipe::NativeNegate => {
                self.emit_wasm_opcode_operands(mnemonic, operand_type, arguments);
                numeric::emit_opcode(opcode, &mut self.code);
            }
        }
        let result_type = wasm_stack_type(instruction.result_type);
        match output {
            None => WasmOpcode::Drop.encode(&mut self.code),
            Some(output) => {
                self.validate_output_slot_type(output, result_type);
                self.assign_output_local(output);
            }
        }
        Ok(true)
    }

    /// 按自然栈类型发射绑定实参，并断言与计划冻结的实参载体一致。
    fn emit_wasm_opcode_operands(&mut self, mnemonic: &str, expected: u8, arguments: &[MirOperand]) {
        for argument in arguments {
            let actual = self.operand_wasm_stack_type(argument);
            assert_eq!(
                actual, expected,
                "WASM `wasm_opcode` 实参栈类型与目标绑定不一致: `{mnemonic}` expects {expected:#x} actual {actual:#x} in {}",
                self.mir_fn.symbol
            );
            self.emit_operand(argument);
        }
    }

    pub(super) fn resolve_callee_import_index(&self, callee: &MirOperand, _arguments: &[MirOperand]) -> Option<u32> {
        let MirOperand::Item(instance) = callee
        else {
            return None;
        };
        self.callee_import_index.get(instance).copied()
    }

    /// `i64` → wasm-gc struct `[i64]`（anyref），供泛型 `T` 数组槽使用。
    fn emit_box_i64_payload(&mut self, operand: &MirOperand) {
        let box_ty = self.gc_i64_box_type_index;
        self.emit_struct_new_default(box_ty);
        let tmp = self.alloc_anyref_local();
        self.emit_local_set(tmp);
        self.emit_local_get(tmp);
        self.emit_ref_cast_struct(box_ty);
        self.emit_operand(operand);
        self.emit_struct_set(box_ty, 0);
        self.emit_local_get(tmp);
    }

    /// 栈顶 anyref（i64 box）→ i64。
    ///
    /// 载荷是 null 时不得回落 `i64 0`：缺值不是有效值。取消 null 分支后，
    /// `ref.cast (ref $i64_box)` 在 null 上执行即 trap，
    /// 由上游 boxing / `ArrayNew` default 合同保证载荷已初始化。
    pub(super) fn emit_unbox_i64_payload(&mut self) {
        let box_ty = self.gc_i64_box_type_index;
        self.emit_ref_cast_struct(box_ty);
        self.emit_struct_get(box_ty, 0);
    }

    /// p3 `write-via-stream`：utf8 线性句?→?`stream.new` / write / drop →?宿主?
    ///
    /// 顺序必须是：new →??reader 交给 write-via-stream →?write(writer) →?drop-writable →?drop future?
    /// ?write 再交 reader 会在 sync `stream.write` 上永久挂起?
    fn try_emit_wasi_cli_write_via_stream(&mut self, arguments: &[MirOperand], import_index: u32, output: Option<MirValueRef>) -> bool {
        if !self.wasi_mode || arguments.len() != 1 {
            return false;
        }
        let Some((module, field)) = self.host_imports.get(import_index as usize)
        else {
            return false;
        };
        if field != "write-via-stream" {
            return false;
        }
        if !(module.contains("stdout") || module.contains("stderr")) {
            return false;
        }
        let Some(stream_new) = self.find_host_import_index(module, "[stream-new-0]write-via-stream")
        else {
            return false;
        };
        let Some(stream_write) = self.find_host_import_index(module, "[stream-write-0]write-via-stream")
        else {
            return false;
        };
        let Some(stream_drop_w) = self.find_host_import_index(module, "[stream-drop-writable-0]write-via-stream")
        else {
            return false;
        };
        let Some(future_drop) = self.find_host_import_index(module, "[future-drop-readable-1]write-via-stream")
        else {
            return false;
        };
        // utf8 handle: [len:u32 LE][bytes…]
        let handle = self.alloc_i32_local();
        let len = self.alloc_i32_local();
        let bytes_ptr = self.alloc_i32_local();
        let pair = self.alloc_i64_local();
        let writer = self.alloc_i32_local();
        let reader = self.alloc_i32_local();
        let fut = self.alloc_i32_local();

        self.emit_operand_coerced(&arguments[0], VALTYPE_I32);
        self.emit_local_set(handle);

        self.emit_local_get(handle);
        encode_i32_load(2, 0, &mut self.code);
        self.emit_local_set(len);

        self.emit_local_get(handle);
        self.emit_i32_const(4);
        self.emit_i32_add();
        self.emit_local_set(bytes_ptr);

        self.emit_call(stream_new);
        self.emit_local_set(pair);

        // reader = low32, writer = high32（与 wit-bindgen raw_stream_new 同构?
        self.emit_local_get(pair);
        WasmOpcode::I32WrapI64.encode(&mut self.code);
        self.emit_local_set(reader);
        self.emit_local_get(pair);
        WasmOpcode::I64Const.encode(&mut self.code);
        encode_sleb128_i64(32, &mut self.code);
        WasmOpcode::I64ShrU.encode(&mut self.code);
        WasmOpcode::I32WrapI64.encode(&mut self.code);
        self.emit_local_set(writer);

        self.emit_local_get(reader);
        self.emit_call(import_index);
        self.emit_local_set(fut);

        self.emit_local_get(writer);
        self.emit_local_get(bytes_ptr);
        self.emit_local_get(len);
        self.emit_call(stream_write);
        WasmOpcode::Drop.encode(&mut self.code);

        self.emit_local_get(writer);
        self.emit_call(stream_drop_w);

        self.emit_local_get(fut);
        self.emit_call(future_drop);

        if let Some(out) = output {
            // console write →?unit；void≠unit，用 anyref null 占位?
            self.validate_output_slot_type(out, VALTYPE_ANYREF);
            self.emit_ref_null_anyref();
            self.assign_output_local(out);
        }
        true
    }

    fn find_host_import_index(&self, module: &str, field: &str) -> Option<u32> {
        self.host_imports.iter().position(|(m, f)| m == module && f == field).map(|index| index as u32)
    }
}
