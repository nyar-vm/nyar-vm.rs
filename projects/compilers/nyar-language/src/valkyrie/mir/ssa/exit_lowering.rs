use super::{MirBlockRef, MirBuilder, MirOperand, MirTerminator};
use crate::hir::is_nullable_type;

/// Unified function exit semantics for explicit `return` and `?` early exit.
pub(super) enum FunctionExitKind {
    Return(MirOperand),
    TryScopeExit { target: MirBlockRef, arguments: Vec<MirOperand> },
}

impl MirBuilder {
    pub(super) fn terminate_function_exit(&mut self, kind: FunctionExitKind) {
        match kind {
            FunctionExitKind::Return(value) => {
                self.terminate(MirTerminator::Return { value: Some(value) });
            }
            FunctionExitKind::TryScopeExit { target, arguments } => {
                self.terminate(MirTerminator::Jump { target, arguments });
            }
        }
    }

    pub(super) fn lower_explicit_return(&mut self, value: Option<MirOperand>) {
        self.terminate(MirTerminator::Return { value });
    }

    pub(super) fn nullable_early_exit_operand(&mut self, value: MirOperand) -> MirOperand {
        if is_nullable_type(&self.current_return_type) {
            value
        }
        else {
            unreachable!("nullable early exit requires a structured Nullable return type")
        }
    }

    pub(super) fn terminate_nullable_try_early_exit(&mut self, value: MirOperand) {
        if let Some(try_scope) = self.control_flow.current_try_scope().cloned() {
            let arguments = if try_scope.exit_value.is_some() { vec![value.clone()] } else { Vec::new() };
            self.terminate_function_exit(FunctionExitKind::TryScopeExit { target: try_scope.exit, arguments });
        }
        else {
            let early_return = self.nullable_early_exit_operand(value);
            self.terminate_function_exit(FunctionExitKind::Return(early_return));
        }
    }
}
