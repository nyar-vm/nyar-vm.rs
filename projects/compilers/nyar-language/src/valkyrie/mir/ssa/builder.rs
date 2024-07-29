//! `MirBuilder`：函数体 lowering 的状态、绑定与效应上下文。

use super::{
    MirSumDeclaration, callable_types::CallableReturnTypes, control_flow_context::MirBuilderControlFlow, model::*,
    value_semantics::AggregateLayoutPlan,
};
use crate::types::{
    Identifier, NamePath,
    hir::{HirExpr, HirFunction, HirPattern, HirStatement, HirStatementKind, ValkyrieType},
};
use nyar_types::VariantId;
use std::collections::BTreeMap;

/// `MIR` 函数体 lowering 构建器，维护当前块、绑定与效应上下文。
pub(super) struct MirBuilder {
    pub(super) entry: MirBlockRef,
    pub(super) current_block: MirBlockRef,
    pub(super) current_label: String,
    pub(super) values: Vec<MirValue>,
    pub(super) instructions: Vec<MirInstruction>,
    pub(super) blocks: Vec<MirBlock>,
    pub(super) suspend_points: Vec<MirSuspendPoint>,
    pub(super) continuations: Vec<MirContinuation>,
    pub(super) case_chains: Vec<MirCaseChain>,
    pub(super) bindings: BTreeMap<String, MirOperand>,
    pub(super) value_types: BTreeMap<MirValueRef, ValkyrieType>,
    pub(super) field_declarations: Vec<MirStruct>,
    pub(super) aggregate_layouts: AggregateLayoutPlan,
    /// sum 的语义声明；实例化使用声明 binder，不以布局恢复类型。
    pub(super) sum_types: Vec<MirSumDeclaration>,
    /// 模块内唯一可解析的 Option sum 声明；歧义或缺失则为 `None`（fail-closed）。
    pub(super) option_declaration: Option<nyar_types::ItemId>,
    /// 模块内唯一可解析的 Result sum 声明；歧义或缺失则为 `None`（fail-closed）。
    pub(super) result_declaration: Option<nyar_types::ItemId>,
    pub(super) singleton_accessor_instances: BTreeMap<String, nyar_types::ItemInstanceId>,
    pub(super) static_bindings: BTreeMap<String, HirExpr>,
    pub(super) terminator: Option<MirTerminator>,
    pub(super) value_seed: u32,
    /// 稠密 InstructionId 分配器。
    pub(super) instruction_seed: u32,
    /// 稠密 ProvenanceId 分配器。
    pub(super) provenance_seed: u32,
    pub(super) state_seed: u32,
    /// Unified loop / try / fallthrough scope tracking for lowering.
    pub(super) control_flow: MirBuilderControlFlow,
    /// 嵌套 handler arm 深度：位于 catch/handler 体内时递增。
    /// Arm 内 `raise` 需要绑定到当前 handler；深度为 0 表示不在任何 handler 内。
    pub(super) suspended_handler_depth: usize,
    /// 当前函数返回类型；用于 `?` 传播等需要对照返回类型的路径。
    pub(super) current_return_type: ValkyrieType,
    /// `MIR` lowering 过程中收集的编译期诊断。
    pub(super) diagnostics: Vec<MirDiagnostic>,
    /// 模块级 `Effectful::Resume` 关联类型映射（载体类型名 → Resume 类型）。
    ///
    /// 由 [`collect_effectful_resume_map`] 在 lowering 入口预先构造，
    /// 供 `raise` 路径查询用户 effect 载体的 resume 类型，以正确推断 resume 参数类型。
    pub(super) effectful_resume_map: BTreeMap<nyar_types::ItemId, ValkyrieType>,
    /// 可在 `catch` 调用点内联的效应函数（函数名 → 函数定义）。
    ///
    /// 由 [`effect_lowering::collect_effectful_inline_targets`] 在 lowering 入口预先构造。
    /// 仅包含含 `raise`、无 `return`、非递归的模块级函数。`lower_catch_expr` 在调用点
    /// 检查被调用函数是否在此映射中，若是则内联其函数体以实现跨函数 `raise` 传播。
    pub(super) effectful_inline_targets: BTreeMap<String, HirFunction>,
    /// HIR 冻结的 callable 返回类型；`Call` SSA 结果须与 MIR 定义签名一致。
    pub(super) callable_returns: CallableReturnTypes,
    /// 声明期冻结的成员分派表；模式比较与延迟要求共用。
    pub(super) member_dispatch: Vec<super::generic_call_instantiation::MirDispatchEntry>,
    /// 当前 impl 方法的 owner 类型名（如 `FilterIterator`）。
    ///
    /// 用于把 `self` 参数的 `AutoType`/`SelfType` 解析为具体命名类型，
    /// 使 `field_type_for_object_operand` 能查到泛型结构体字段类型。
    /// 自由函数为 `None`。
    pub(super) impl_owner_type: Option<ValkyrieType>,
    /// 当前 impl 块声明的泛型实参占位（如 `imply Option<T>` → `[T]`）。
    pub(super) impl_type_arguments: Vec<ValkyrieType>,
    /// `[workload_phase]` 解析出的阶段名；函数离开路径插入 `end_phase`。
    pub(super) workload_phase: Option<String>,
}

/// 模块内按简单名唯一解析 sum 声明 ItemId；歧义/缺失返回 `None`（不得猜测）。
impl MirBuilder {
    pub(super) fn sum_identity_by_declarations(
        &mut self,
        owner: nyar_types::ItemId,
        variant: nyar_types::ItemId,
    ) -> Option<(nyar_types::NominalInstanceId, VariantId)> {
        for sum in &self.sum_types {
            if sum.declaration == Some(owner) {
                if let Some(candidate) = sum.variants.iter().find(|item| item.declaration == Some(variant)) {
                    return Some((sum.nominal, candidate.id));
                }
            }
        }
        self.diagnostics.push(MirDiagnostic::UnresolvedVariantIdentity { sum_type: owner.to_string(), variant: variant.to_string() });
        None
    }

    pub(super) fn new(
        aggregate_layouts: AggregateLayoutPlan,
        singleton_accessor_instances: BTreeMap<String, nyar_types::ItemInstanceId>,
        effectful_resume_map: BTreeMap<nyar_types::ItemId, ValkyrieType>,
        effectful_inline_targets: BTreeMap<String, HirFunction>,
        callable_returns: CallableReturnTypes,
        member_dispatch: Vec<super::generic_call_instantiation::MirDispatchEntry>,
        impl_owner_type: Option<ValkyrieType>,
    ) -> Self {
        let entry = MirBlockRef(0);
        Self {
            entry,
            current_block: entry,
            current_label: "entry".to_string(),
            values: Vec::new(),
            instructions: Vec::new(),
            blocks: vec![MirBlock {
                id: entry,
                label: "entry".to_string(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: MirTerminator::Unreachable,
            }],
            suspend_points: Vec::new(),
            continuations: Vec::new(),
            case_chains: Vec::new(),
            bindings: BTreeMap::new(),
            value_types: BTreeMap::new(),
            field_declarations: Vec::new(),
            aggregate_layouts,
            sum_types: Vec::new(),
            option_declaration: None,
            result_declaration: None,
            singleton_accessor_instances,
            static_bindings: BTreeMap::new(),
            terminator: None,
            value_seed: 0,
            instruction_seed: 0,
            provenance_seed: 0,
            state_seed: 0,
            suspended_handler_depth: 0,
            control_flow: MirBuilderControlFlow::default(),
            current_return_type: ValkyrieType::Unit,
            diagnostics: Vec::new(),
            effectful_resume_map,
            effectful_inline_targets,
            callable_returns,
            member_dispatch,
            impl_owner_type,
            impl_type_arguments: Vec::new(),
            workload_phase: None,
        }
    }

    pub(super) fn declared_return_type_for_call(
        &self,
        callee: &MirOperand,
        resolved: Option<&crate::types::hir::HirResolvedCall>,
    ) -> Option<ValkyrieType> {
        if let MirOperand::Callable(instance) = callee {
            if let Some(return_type) = self.callable_returns.by_instance.get(instance) {
                return Some(return_type.clone());
            }
        }
        if let Some(call) = resolved {
            if let Some(instance) = call.instance {
                if let Some(return_type) = self.callable_returns.by_instance.get(&instance) {
                    return Some(return_type.clone());
                }
            }
        }
        if let MirOperand::Symbol(path) = callee {
            // 只按限定符号取签名；末段短名不是身份，不得回落认亲。
            if let Some(return_type) = self.callable_returns.by_symbol.get(&path.to_string()) {
                return Some(return_type.clone());
            }
        }
        resolved.and_then(|call| self.callable_returns.by_symbol.get(&call.symbol.to_string()).cloned())
    }

    pub(super) fn return_type_for_resolved_call(&self, callee: &MirOperand, resolved: &crate::types::hir::HirResolvedCall) -> ValkyrieType {
        match &resolved.return_type {
            ValkyrieType::AutoType => self.declared_return_type_for_call(callee, Some(resolved)).unwrap_or(ValkyrieType::Unit),
            return_type => return_type.clone(),
        }
    }

    pub(super) fn next_value(&mut self, origin: MirValueOrigin) -> MirValueRef {
        let id = MirValueRef(self.value_seed);
        self.value_seed += 1;
        self.values.push(MirValue { id, origin });
        id
    }

    /// 推入指令并分配新的稠密 [`InstructionId`] + [`ProvenanceId`]。
    pub(super) fn push_instruction(&mut self, operation: MirOperation, results: Vec<MirValueRef>) {
        let id = nyar_types::InstructionId::from_index(self.instruction_seed).expect("InstructionId");
        self.instruction_seed = self.instruction_seed.saturating_add(1);
        let provenance = nyar_types::ProvenanceId::from_index(self.provenance_seed).expect("ProvenanceId");
        self.provenance_seed = self.provenance_seed.saturating_add(1);
        self.instructions.push(MirInstruction { id, results, kind: operation, provenance });
    }

    /// Emit `Call` and bind its SSA result (`instruction.results[0]`).
    pub(super) fn push_call(&mut self, callee: MirOperand, arguments: Vec<MirOperand>) -> MirValueRef {
        let value = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(MirOperation::Call { callee, arguments }, vec![value]);
        value
    }

    /// Emit a call with known return type. `unit` 不得占用物理 value 槽（BPHYS001）。
    pub(super) fn push_call_returning(&mut self, callee: MirOperand, arguments: Vec<MirOperand>, return_type: ValkyrieType) -> MirOperand {
        if matches!(return_type, ValkyrieType::Unit) {
            self.push_instruction(MirOperation::Call { callee, arguments }, Vec::new());
            return MirOperand::Constant(MirConstant::Unit);
        }
        let value = self.push_call(callee, arguments);
        self.value_types.insert(value, return_type);
        MirOperand::Value(value)
    }

    pub(super) fn next_state_id(&mut self) -> u32 {
        let id = self.state_seed;
        self.state_seed += 1;
        id
    }

    pub(super) fn terminate(&mut self, terminator: MirTerminator) {
        if self.should_end_workload_phase(&terminator) {
            if let Some(phase) = self.workload_phase.clone() {
                self.emit_phase_host_call("end_phase", &phase);
            }
        }
        self.terminator = Some(terminator);
    }

    /// 哪些终结符表示离开本函数的阶段作用域。
    ///
    /// - `Return` / 不可恢复 `Raise`（`YieldToRuntime`）/ `Unreachable`：关闭阶段
    /// - `Yield` / `DelegateYield`：可 resume，保持阶段
    /// - `PerformEffect`：可能 resume，不在此关闭
    pub(super) fn should_end_workload_phase(&self, terminator: &MirTerminator) -> bool {
        if self.workload_phase.is_none() {
            return false;
        }
        match terminator {
            MirTerminator::Return { .. } | MirTerminator::Unreachable => true,
            MirTerminator::YieldToRuntime { effect: MirEffectKind::Raise, .. } => true,
            MirTerminator::YieldToRuntime {
                effect:
                    MirEffectKind::Yield
                    | MirEffectKind::DelegateYield
                    | MirEffectKind::Await
                    | MirEffectKind::AsyncSpawn
                    | MirEffectKind::AsyncBlock,
                ..
            } => false,
            MirTerminator::PerformEffect { .. }
            | MirTerminator::Jump { .. }
            | MirTerminator::Branch { .. }
            | MirTerminator::StateDispatch { .. } => false,
        }
    }

    /// 发射 `begin_phase` / `end_phase` 宿主调用（符号末段供 nyar_vm emitter 识别）。
    pub(super) fn emit_phase_host_call(&mut self, host_symbol: &str, phase: &str) {
        let constant = self.next_value(MirValueOrigin::Literal);
        self.value_types.insert(constant, ValkyrieType::Utf8);
        self.push_instruction(
            MirOperation::LoadConstant { constant: MirConstant::Utf8(phase.to_string()), ty: Some(ValkyrieType::Utf8) },
            vec![constant],
        );
        let _ = self.push_call(MirOperand::Symbol(NamePath::new(vec![Identifier::new(host_symbol)])), vec![MirOperand::Value(constant)]);
    }

    pub(super) fn new_block(&mut self, label: &str) -> MirBlockRef {
        let id = MirBlockRef(self.blocks.len() as u32);
        self.blocks.push(MirBlock {
            id,
            label: label.to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: MirTerminator::Unreachable,
        });
        self.current_block = id;
        self.current_label = label.to_string();
        id
    }

    pub(super) fn flush_block(&mut self, label: &str) {
        // 将缓冲中的指令写入 current_block；块 ID 保持不变，仅更新标签与内容。
        let block = &mut self.blocks[self.current_block.0 as usize];
        block.label = label.to_string();
        block.instructions = self.instructions.clone();
        block.terminator = self.terminator.clone().unwrap_or(MirTerminator::Unreachable);
        // 清空指令缓冲与终结符，准备写入下一个块。
        self.instructions.clear();
        self.terminator = None;
    }

    pub(super) fn ensure_loop_exit_parameter(&mut self, loop_index: usize, ty: Option<ValkyrieType>) -> MirValueRef {
        if let Some(value) = self.control_flow.loop_at(loop_index).exit_value {
            return value;
        }

        let exit_block = self.control_flow.loop_at(loop_index).exit;
        if let Some(value) = self.blocks[exit_block.0 as usize].parameters.first().copied() {
            if let Some(ty) = ty {
                self.value_types.entry(value).or_insert(ty);
            }
            self.control_flow.loop_at_mut(loop_index).exit_value = Some(value);
            return value;
        }

        let value = self.next_value(MirValueOrigin::Temporary);
        if let Some(ty) = ty {
            self.value_types.insert(value, ty);
        }
        self.blocks[exit_block.0 as usize].parameters.push(value);
        self.control_flow.loop_at_mut(loop_index).exit_value = Some(value);
        value
    }

    pub(super) fn ensure_block_parameter(&mut self, block: MirBlockRef, name: &str, ty: Option<ValkyrieType>) -> MirValueRef {
        if let Some(value) = self.blocks[block.0 as usize].parameters.first().copied() {
            if let Some(ty) = ty {
                self.value_types.entry(value).or_insert(ty);
            }
            return value;
        }

        let value = self.next_value(MirValueOrigin::BlockParameter { block, name: name.to_string() });
        if let Some(ty) = ty {
            self.value_types.insert(value, ty);
        }
        self.blocks[block.0 as usize].parameters.push(value);
        value
    }

    pub(super) fn ensure_handler_exit_parameter(&mut self, handler_index: usize, ty: Option<ValkyrieType>) -> MirValueRef {
        if let Some(value) = self.control_flow.handler_at(handler_index).exit_value {
            return value;
        }

        let exit_block = self.control_flow.handler_at(handler_index).exit;
        if let Some(value) = self.blocks[exit_block.0 as usize].parameters.first().copied() {
            if let Some(ty) = ty {
                self.value_types.entry(value).or_insert(ty);
            }
            self.control_flow.handler_at_mut(handler_index).exit_value = Some(value);
            return value;
        }

        let value = self.next_value(MirValueOrigin::Temporary);
        if let Some(ty) = ty {
            self.value_types.insert(value, ty);
        }
        self.blocks[exit_block.0 as usize].parameters.push(value);
        self.control_flow.handler_at_mut(handler_index).exit_value = Some(value);
        value
    }

    pub(super) fn resolve_loop_index(&self, label: Option<&Identifier>) -> Option<usize> {
        self.control_flow.resolve_loop_index(label)
    }

    pub(super) fn bind_catch_arm_pattern(&mut self, pattern: &HirPattern, payload: MirOperand, extractor_payload: Option<MirOperand>) {
        match pattern {
            HirPattern::Else | HirPattern::Wildcard => {}
            _ => self.bind_pattern_from_operand_with_payload(pattern, payload, None, extractor_payload),
        }
    }

    pub(super) fn lower_statement(&mut self, statement: &HirStatement) {
        match &statement.kind {
            HirStatementKind::Let { pattern, initializer, ty, .. } => {
                self.record_static_binding(pattern, initializer.as_deref());
                if let Some(expr) = initializer.as_deref() {
                    self.bind_pattern_from_expr(pattern, expr, ty.clone());
                }
                else {
                    self.bind_pattern_from_operand(pattern, MirOperand::Constant(MirConstant::Unit), ty.clone());
                }
            }
            HirStatementKind::Expr(expression) => {
                let _ = self.lower_expr_to_operand(expression);
            }
        }
    }

    pub(super) fn reject_unmodeled_static_operation(&mut self, operation: &'static str, origin: MirValueOrigin) -> MirValueRef {
        self.diagnostics.push(MirDiagnostic::UnresolvedCallableIdentity { symbol: operation.to_string() });
        self.next_value(origin)
    }
}
