//! Split from former monolithic wasm mir lowerer (ADR 0008).
#![allow(deprecated)]

#[allow(deprecated)]
use super::*;

/// Transitional Wasm prepare/encode lowerer for Semantic MIR executables.
///
/// Historical name; prefer speaking of WasmModuleModel encoding, not “Wasm MIR”.
#[deprecated(note = "historical name; Semantic MIR → WasmModuleModel encode (not Wasm MIR)")]
pub(crate) struct WasmMirLowerer<'a> {
    pub(crate) ctx: &'a ExecutableLoweringContext<'a>,
    pub(crate) mir_fn: &'a MirFunction,
    /// 当前函数?WASM 返回值类型字节?
    /// `None` = void（无返回值）；`Some(VALTYPE_I32)` = i32；`Some(VALTYPE_ANYREF)` = anyref?
    /// `Some(VALTYPE_F64)` = f64；`Some(VALTYPE_I64)` = i64?
    pub(crate) return_value_type: Option<u8>,
    /// Node JS-glue 路径：utf8/utf16 ?anyref 传递（宿主字符串）?
    pub(crate) js_glue_utf8_as_anyref: bool,
    /// WASI 轨：字符串字面量作为 i32 偏移量传递，存入 data 段?
    pub(crate) wasi_mode: bool,
    pub(crate) code: Vec<u8>,
    /// Scratch: aligned pointer returned by the global bump allocator.
    pub(crate) stack_ptr_local: u32,
    /// Scratch: `new_end` after bump.
    pub(crate) bump_end_local: u32,
    /// Scratch: pages to grow.
    pub(crate) bump_pages_local: u32,
    pub(crate) next_local: u32,
    /// 每个声明 local 的完?valtype 字节（i32=VALTYPE_I32, anyref=VALTYPE_ANYREF, struct ref=VALTYPE_REF+typeidx）?
    pub(crate) local_valtypes: Vec<Vec<u8>>,
    pub(crate) value_locals: BTreeMap<MirValueRef, u32>,
    /// 引用类型 local 的集合。这?local 的值类型是 `anyref` (VALTYPE_ANYREF),而非 i32?
    pub(crate) reference_locals: BTreeMap<MirValueRef, u32>,
    pub(crate) var_locals: BTreeMap<String, u32>,
    pub(crate) scalar_locals: BTreeMap<MirValueRef, u32>,
    pub(crate) block_order: Vec<MirBlockRef>,
    pub(crate) block_index: BTreeMap<MirBlockRef, usize>,
    pub(crate) function_index_by_name: &'a BTreeMap<String, u32>,
    pub(crate) type_index_by_name: &'a BTreeMap<String, u32>,
    pub(crate) param_types_by_name: &'a BTreeMap<String, Vec<u8>>,
    pub(crate) return_types_by_name: &'a BTreeMap<String, Option<u8>>,
    /// ?`function_index_by_name` 下标对齐?callee 形参 wasm 类型（call 实参 coerce 权威来源）?
    pub(crate) param_types_by_function_index: &'a BTreeMap<u32, Vec<u8>>,
    pub(crate) return_types_by_function_index: &'a BTreeMap<u32, Option<u8>>,
    pub(crate) import_param_types: &'a [Vec<u8>],
    pub(crate) import_return_types: &'a [Option<u8>],
    /// 引用类型 layout_id -> wasm-gc structtype ?type_index?
    pub(crate) gc_struct_type_indices: &'a BTreeMap<LayoutId, u32>,
    /// unite sum_name -> wasm-gc structtype [i32,anyref] ?type_index?
    pub(crate) gc_sum_type_indices: &'a BTreeMap<String, u32>,
    /// Fine/Fail ?unite 标量 payload（utf8/bool/i32）装箱用 structtype [i32]?
    pub(crate) gc_i32_box_type_index: u32,
    /// heap array element_type 字符串键 -> wasm-gc arraytype ?type_index?
    pub(crate) gc_array_type_indices: &'a BTreeMap<String, u32>,
    pub(crate) callee_import_index: &'a BTreeMap<String, u32>,
    /// WASI 宿主 import 表（?stream 内建），?write-via-stream 协议查找索引?
    pub(crate) host_imports: &'a [(String, String)],
    pub(crate) string_literal_index: &'a BTreeMap<String, u32>,
    /// WASI 轨：字符串字面量到线性内存偏移量的映射?
    pub(crate) string_literal_offset: &'a BTreeMap<String, u32>,
    pub(crate) const_utf8_import: Option<u32>,
    /// 需要在函数入口显式初始化的 typed ref local 列表 (local_index, type_index)?
    /// V8 要求 `(ref null T)` typed local 必须在入口显?`ref.null T` + `local.set` 初始化，
    /// 否则?"uninitialized non-defaultable local" 编译错误?
    pub(crate) typed_ref_locals_to_init: Vec<(u32, u32)>,
    /// CFG 调度用的基本?PC（`block_order` 下标）。`loop` + `br_table` 按此分发?
    /// 正确实现前向 Jump、后?Jump ?Branch；旧线?`br 0` 模型会误落入错误后继
    /// （自?`build` →?VON lexer 早退 `Fine([])` →?`ref.cast` illegal cast）?
    pub(crate) pc_local: u32,
}


#[allow(deprecated)]
impl<'a> WasmMirLowerer<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        ctx: &'a ExecutableLoweringContext<'a>,
        mir_fn: &'a MirFunction,
        return_value_type: Option<u8>,
        js_glue_utf8_as_anyref: bool,
        wasi_mode: bool,
        function_index_by_name: &'a BTreeMap<String, u32>,
        type_index_by_name: &'a BTreeMap<String, u32>,
        param_types_by_name: &'a BTreeMap<String, Vec<u8>>,
        return_types_by_name: &'a BTreeMap<String, Option<u8>>,
        param_types_by_function_index: &'a BTreeMap<u32, Vec<u8>>,
        return_types_by_function_index: &'a BTreeMap<u32, Option<u8>>,
        import_param_types: &'a [Vec<u8>],
        import_return_types: &'a [Option<u8>],
        gc_struct_type_indices: &'a BTreeMap<LayoutId, u32>,
        gc_array_type_indices: &'a BTreeMap<String, u32>,
        gc_sum_type_indices: &'a BTreeMap<String, u32>,
        gc_i32_box_type_index: u32,
        callee_import_index: &'a BTreeMap<String, u32>,
        host_imports: &'a [(String, String)],
        string_literal_index: &'a BTreeMap<String, u32>,
        string_literal_offset: &'a BTreeMap<String, u32>,
        const_utf8_import: Option<u32>,
    ) -> Self {
        let block_order = collect_reachable_blocks(mir_fn);
        let block_index = block_order.iter().enumerate().map(|(index, block_id)| (*block_id, index)).collect();
        // wasm 函数参数占据 local 0..param_count-1，声明局部从 param_count 开始?
        // stack_ptr 是第一个声明的局部，位于 local param_count?
        // 若函数有 anyref 参数，旧代码?stack_ptr 放在 local 0 会与参数 0 冲突?
        // 导致 V8 ?`local.set[0] expected type anyref, found i32.const`?
        let param_count = mir_fn.param_types.len() as u32;
        let mut lowerer = Self {
            ctx,
            mir_fn,
            return_value_type,
            js_glue_utf8_as_anyref,
            wasi_mode,
            code: Vec::new(),
            stack_ptr_local: param_count,
            bump_end_local: param_count + 1,
            bump_pages_local: param_count + 2,
            // Branch flag locals 按嵌套深度按需 alloc_i32_local，不预留单一 slot?
            next_local: param_count + 3,
            local_valtypes: vec![vec![VALTYPE_I32], vec![VALTYPE_I32], vec![VALTYPE_I32]],
            value_locals: BTreeMap::new(),
            reference_locals: BTreeMap::new(),
            var_locals: BTreeMap::new(),
            scalar_locals: BTreeMap::new(),
            block_order,
            block_index,
            function_index_by_name,
            type_index_by_name,
            param_types_by_name,
            return_types_by_name,
            param_types_by_function_index,
            return_types_by_function_index,
            import_param_types,
            import_return_types,
            gc_struct_type_indices,
            gc_array_type_indices,
            gc_sum_type_indices,
            gc_i32_box_type_index,
            callee_import_index,
            host_imports,
            string_literal_index,
            string_literal_offset,
            const_utf8_import,
            typed_ref_locals_to_init: Vec::new(),
            pc_local: 0,
        };
        lowerer.pc_local = lowerer.alloc_i32_local();
        // Linear bump uses the module-global heap cursor (never STACK_BASE=0).
        for block_id in lowerer.block_order.clone() {
            let Some(block) = mir_fn.blocks.get(block_id.0 as usize)
            else {
                continue;
            };
            // 入口块的参数对应函数参数，它们已?local 0..param_count-1?
            // 直接映射到参?local，避免重复分配导致参数值丢失?
            // 使用 param_types 判定引用类型，而非 value_types（后者可能缺条目导致误判）?
            let is_entry_block = block.id == mir_fn.entry;
            for (param_slot, param) in block.parameters.iter().enumerate() {
                if is_entry_block && param_slot < mir_fn.param_types.len() {
                    let local = param_slot as u32;
                    let param_ty = &mir_fn.param_types[param_slot];
                    // 与函数签?`wasm_param_value_type_for` 对齐：GC struct / Reference →?anyref?
                    // 若只?`storage_for_type` 而签名因 gc_struct 登记?anyref，会?param
                    // 误写?value_locals，随?`local.get` ?anyref、`local.set` 却按 i32?
                    let param_vt = wasm_param_value_type_for(ctx, param_ty, gc_struct_type_indices, js_glue_utf8_as_anyref);
                    let is_reference = param_vt == WASM_GC_ANYREF || param_vt == WASM_GC_EXTERNREF;
                    if is_reference {
                        lowerer.reference_locals.insert(*param, local);
                    }
                    else {
                        lowerer.value_locals.insert(*param, local);
                    }
                    continue;
                }
                // 非入口块参数：分配新 local?
                // 引用 →?anyref；标量按 value_types 的真?wasm valtype（含 i64/f64），
                // 禁止一?i32（否?jump/Copy ?`local.set expected i32, found i64`）?
                let param_vt = mir_fn
                    .value_types
                    .get(param)
                    .map(|ty| wasm_param_value_type_for(ctx, ty, gc_struct_type_indices, js_glue_utf8_as_anyref))
                    .unwrap_or(VALTYPE_I32);
                if param_vt == WASM_GC_ANYREF || param_vt == WASM_GC_EXTERNREF {
                    let local = lowerer.alloc_anyref_local();
                    lowerer.reference_locals.insert(*param, local);
                }
                else {
                    let local = lowerer.alloc_scalar_local_for_stack_type(param_vt);
                    lowerer.value_locals.insert(*param, local);
                }
            }
            for instruction in &block.instructions {
                lowerer.plan_instruction(instruction);
            }
        }
        lowerer
    }

    pub(crate) fn storage_for_type(&self, ty: &NyarType) -> MirStorageKind {
        mir_storage_for_type(self.ctx, ty, self.js_glue_utf8_as_anyref)
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        // ?local index 顺序扫描,将相同类型的连续 local 合并为一组?
        // wasm 规范要求 local 声明组的总数等于实际 local ?顺序?local 索引?
        let mut locals: Vec<(u32, Vec<u8>)> = Vec::new();
        let mut index = 0u32;
        while index < self.local_valtypes.len() as u32 {
            let ty = &self.local_valtypes[index as usize];
            let mut count = 1u32;
            while (index + count) < self.local_valtypes.len() as u32 && self.local_valtypes[(index + count) as usize] == *ty {
                count += 1;
            }
            locals.push((count, ty.clone()));
            index += count;
        }
        let mut body = Vec::new();
        encode_uleb128(locals.len() as u32, &mut body);
        for (count, ty) in locals {
            encode_uleb128(count, &mut body);
            body.extend_from_slice(&ty);
        }
        body.extend_from_slice(&self.code);
        WasmOpcode::End.encode(&mut body);
        body
    }

    pub(crate) fn alloc_i32_local(&mut self) -> u32 {
        let local = self.next_local;
        self.next_local += 1;
        self.local_valtypes.push(vec![VALTYPE_I32]);
        local
    }

    pub(crate) fn alloc_i64_local(&mut self) -> u32 {
        let local = self.next_local;
        self.next_local += 1;
        self.local_valtypes.push(vec![VALTYPE_I64]);
        local
    }

    pub(crate) fn alloc_f64_local(&mut self) -> u32 {
        let local = self.next_local;
        self.next_local += 1;
        self.local_valtypes.push(vec![VALTYPE_F64]);
        local
    }

    /// 按栈上标?valtype 分配 local（i64/f64/i32）。引用类型请?`alloc_anyref_local`?
    pub(crate) fn alloc_scalar_local_for_stack_type(&mut self, stack_ty: u8) -> u32 {
        match stack_ty {
            VALTYPE_I64 => self.alloc_i64_local(),
            VALTYPE_F64 => self.alloc_f64_local(),
            _ => self.alloc_i32_local(),
        }
    }

    /// 分配 typed `(ref null T)` local，供 struct.get/set 使用?
    pub(crate) fn alloc_struct_ref_local(&mut self, type_index: u32) -> u32 {
        let _ = type_index;
        return self.alloc_anyref_local();
        /*
        let local = self.next_local;
        self.next_local += 1;
        let mut valtype = vec![VALTYPE_REF];
        // heaptype 使用 signed LEB128；ULEB128 ?type_index >= 64 时会与抽?heap type 冲突
        //（例?124 →?VALTYPE_F64 ?V8 读成 -4/f64，触?`Unknown heap type -4`）?
        encode_sleb128_i32(i32::try_from(type_index).unwrap_or(i32::MAX), &mut valtype);
        self.local_valtypes.push(valtype);
        self.typed_ref_locals_to_init.push((local, type_index));
        local */
    }

    /// 分配一?anyref local。该 local ?`finish()` 中按?local index 自动归入 anyref 组?
    pub(crate) fn alloc_anyref_local(&mut self) -> u32 {
        let local = self.next_local;
        self.next_local += 1;
        self.local_valtypes.push(vec![WASM_GC_ANYREF]);
        local
    }

    /// ?wasm local 绝对索引映射为值类型字节?
    ///
    /// `local_valtypes` 仅覆盖声明局部（?`stack_ptr_local` 起）?
    /// 函数参数类型来自 `mir_fn.param_types`?
    pub(crate) fn wasm_local_value_type(&self, local_index: u32) -> u8 {
        let param_count = self.stack_ptr_local;
        if local_index < param_count {
            return match self.mir_fn.param_types.get(local_index as usize) {
                Some(ty) => {
                    let vt = wasm_param_value_type_for(self.ctx, ty, self.gc_struct_type_indices, self.js_glue_utf8_as_anyref);
                    if vt == WASM_GC_ANYREF || vt == WASM_GC_EXTERNREF { WASM_GC_ANYREF } else { vt }
                }
                None => panic!(
                    "WASM emit fail-closed: unknown param local {} in `{}`; refuse I32 default (ADR 0008)",
                    local_index, self.mir_fn.symbol
                ),
            };
        }
        let declared = (local_index - param_count) as usize;
        match self.local_valtypes.get(declared).map(|v| v.as_slice()) {
            Some([VALTYPE_I32]) => VALTYPE_I32,
            // alloc_anyref_local 写入 WASM_GC_ANYREF? VALTYPE_ANYREF）；两者都认?
            Some([VALTYPE_ANYREF]) | Some([WASM_GC_ANYREF]) | Some([VALTYPE_REF, ..]) => WASM_GC_ANYREF,
            Some([VALTYPE_EXTERNREF]) | Some([WASM_GC_EXTERNREF]) => WASM_GC_EXTERNREF,
            Some([VALTYPE_I64]) => VALTYPE_I64,
            Some([VALTYPE_F64]) => VALTYPE_F64,
            other => panic!(
                "WASM emit fail-closed: unknown local valtype {:?} at {} in `{}`; refuse I32 default (ADR 0008)",
                other, local_index, self.mir_fn.symbol
            ),
        }
    }
}
