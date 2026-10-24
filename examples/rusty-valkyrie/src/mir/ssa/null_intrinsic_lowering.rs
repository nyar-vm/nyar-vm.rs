use crate::types::hir::ValkyrieType;
use nyar_types::IntrinsicId;

use super::{MirBuilder, MirOperand, MirOperation, MirValueOrigin};

impl MirBuilder {
    pub(super) fn emit_null_is_null_operand(&mut self, value: MirOperand) -> MirOperand {
        let result = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(MirOperation::NullIsNull { value }, vec![result]);
        self.value_types.insert(result, ValkyrieType::Boolean);
        MirOperand::Value(result)
    }

    pub(super) fn emit_null_unwrap_operand(&mut self, value: MirOperand, payload_type: ValkyrieType) -> MirOperand {
        let result = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(MirOperation::NullUnwrap { value }, vec![result]);
        self.value_types.insert(result, payload_type);
        MirOperand::Value(result)
    }

    /// 尝试把 null intrinsic 调用 lower 为专用 opcode。
    ///
    /// **收紧（batch 7）**：
    /// - intrinsic 身份只能来自 `HirResolvedCall.intrinsic`（由 overload 结构化绑定），
    ///   禁止按符号路径段猜测（`resolve_from_segments` 旁路已删除）。
    /// - intrinsic 必须绑定 adaptor（有 ItemId / ItemInstanceId）才能走专用 opcode；
    ///   语言 builtin intrinsic 候选只挂 intrinsic 身份、没有 declaration/instance，
    ///   因此**必然降级**到普通 Call 路径，由下游 adaptor 消费。
    pub(super) fn try_lower_null_intrinsics(
        &mut self,
        resolved: Option<&crate::types::hir::HirResolvedCall>,
        callee: &MirOperand,
        arguments: &[MirOperand],
    ) -> Option<MirOperand> {
        let intrinsic = Self::callee_intrinsic_id(resolved, callee)?;
        // intrinsic 必须绑定 adaptor 才能走专用 opcode；未绑定降级到 Call 路径
        if !Self::has_intrinsic_adaptor_binding(resolved) {
            return None;
        }
        match intrinsic {
            IntrinsicId::IsNull if arguments.len() == 1 => Some(self.emit_null_is_null_operand(arguments[0].clone())),
            IntrinsicId::UnwrapNull if arguments.len() == 1 => {
                let payload_type = resolved.map(|call| call.return_type.clone()).or_else(|| {
                    super::infer_builder_operand_type(&arguments[0], &self.value_types).and_then(|ty| crate::hir::nullable_payload_type(&ty))
                })?;
                Some(self.emit_null_unwrap_operand(arguments[0].clone(), payload_type))
            }
            _ => None,
        }
    }
}
