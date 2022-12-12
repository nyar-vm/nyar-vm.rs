//! MIR lowering control-flow context: unified label registry + block-level loop/try/fallthrough data.

use std::collections::BTreeMap;

use crate::{
    types::{
        Identifier,
        hir::{HirMatchArm, ValkyrieType},
    },
    valkyrie::control_flow::{ControlFlowContext, ControlFlowScopeKind, ScopeKind, TryScopeData},
};

use super::{MirBlockRef, MirValueRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MirLoopContext {
    pub header: MirBlockRef,
    pub exit: MirBlockRef,
    pub exit_value: Option<MirValueRef>,
    /// Set when `break` (with or without value) targets this loop's exit.
    /// Used to decide whether `loop_exit` is live for `while true` / bare `loop`.
    pub exit_reached_by_break: bool,
    pub carried_values: Vec<String>,
    pub carried_value_refs: BTreeMap<String, MirValueRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MirTryScopeContext {
    pub exit: MirBlockRef,
    pub exit_value: Option<MirValueRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MirCaseChainContext {
    pub next_arm: MirBlockRef,
    pub fallthrough_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MirGeneratorContext {
    pub yield_block: MirBlockRef,
    pub is_async_generator: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MirAsyncContext {
    pub await_block: MirBlockRef,
    pub is_async_fn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MirCatchContext {
    pub raise_block: MirBlockRef,
    pub resume_block: MirBlockRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MirFallthroughContext {
    pub target: MirBlockRef,
}

/// `catch` handler dispatch context tracked by the unified control-flow stack.
///
/// Each entry corresponds to an active `catch { ... }` expression and carries
/// the arms, exit block and lazily-materialised exit value parameter used by
/// handler dispatch lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MirHandlerDispatchContext {
    /// Arms of the `catch` expression, cloned from HIR for dispatch lowering.
    pub arms: Vec<HirMatchArm>,
    /// Exit block jumped to once the handler (or propagated raise) completes.
    pub exit: MirBlockRef,
    /// Lazily-created exit value parameter, shared by all arms that produce a value.
    pub exit_value: Option<MirValueRef>,
}

/// `resume` continuation context tracked by the unified control-flow stack.
///
/// Each entry is pushed while lowering a `catch` arm body so that `resume`
/// expressions can resolve the surrounding continuation target and parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MirResumeContinuationContext {
    /// Index into `MirFunction::continuations` this resume entry belongs to.
    pub continuation: usize,
    /// Block the `resume` expression jumps to.
    pub target: MirBlockRef,
    /// Block parameter that receives the resumed value.
    pub parameter: MirValueRef,
    /// Human-readable parameter name used for block-parameter creation.
    pub parameter_name: &'static str,
    /// Inferred resume parameter type, if known.
    pub parameter_type: Option<ValkyrieType>,
}

/// MIR builder control-flow state backed by shared [`ControlFlowContext`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct MirBuilderControlFlow {
    base: ControlFlowContext,
    loops: Vec<MirLoopContext>,
    tries: Vec<MirTryScopeContext>,
    case_chains: Vec<MirCaseChainContext>,
    generators: Vec<MirGeneratorContext>,
    async_contexts: Vec<MirAsyncContext>,
    catches: Vec<MirCatchContext>,
    fallthroughs: Vec<MirFallthroughContext>,
    handlers: Vec<MirHandlerDispatchContext>,
    resumes: Vec<MirResumeContinuationContext>,
}

impl MirBuilderControlFlow {
    pub fn push_loop(&mut self, label: Option<String>, mir: MirLoopContext) {
        self.base.push_loop(label, false);
        self.loops.push(mir);
    }

    pub fn pop_loop(&mut self) -> MirLoopContext {
        self.base.pop_loop();
        self.loops.pop().expect("loop context should exist")
    }

    pub fn push_temp_loop(&mut self, mir: MirLoopContext) {
        self.loops.push(mir);
    }

    pub fn pop_temp_loop(&mut self) -> MirLoopContext {
        self.loops.pop().expect("temp loop context should exist")
    }

    pub fn push_try(&mut self, data: TryScopeData, mir: MirTryScopeContext) {
        self.base.push_try(data);
        self.tries.push(mir);
    }

    pub fn pop_try(&mut self) {
        self.base.pop_try();
        let _ = self.tries.pop();
    }

    pub fn current_try_scope(&self) -> Option<&MirTryScopeContext> {
        self.tries.last()
    }

    pub fn in_try_scope(&self) -> bool {
        self.base.in_try_scope()
    }

    pub fn push_case_chain(&mut self, label: Option<String>, mir: MirCaseChainContext) {
        self.base.push_case_chain(label, None, mir.fallthrough_allowed);
        self.case_chains.push(mir);
    }

    pub fn pop_case_chain(&mut self) -> MirCaseChainContext {
        self.base.pop_case_chain();
        self.case_chains.pop().expect("case chain context should exist")
    }

    pub fn current_case_chain(&self) -> Option<&MirCaseChainContext> {
        self.case_chains.last()
    }

    pub fn push_generator(&mut self, label: Option<String>, mir: MirGeneratorContext) {
        self.base.push_generator(label, mir.is_async_generator);
        self.generators.push(mir);
    }

    pub fn pop_generator(&mut self) -> MirGeneratorContext {
        self.base.pop_generator();
        self.generators.pop().expect("generator context should exist")
    }

    pub fn current_generator(&self) -> Option<&MirGeneratorContext> {
        self.generators.last()
    }

    pub fn push_async(&mut self, label: Option<String>, mir: MirAsyncContext) {
        self.base.push_async(label, mir.is_async_fn);
        self.async_contexts.push(mir);
    }

    pub fn pop_async(&mut self) -> MirAsyncContext {
        self.base.pop_async();
        self.async_contexts.pop().expect("async context should exist")
    }

    pub fn current_async(&self) -> Option<&MirAsyncContext> {
        self.async_contexts.last()
    }

    pub fn push_catch(&mut self, label: Option<String>, mir: MirCatchContext) {
        self.base.push_catch(label, true, true);
        self.catches.push(mir);
    }

    pub fn pop_catch(&mut self) -> MirCatchContext {
        self.base.pop_catch();
        self.catches.pop().expect("catch context should exist")
    }

    pub fn current_catch(&self) -> Option<&MirCatchContext> {
        self.catches.last()
    }

    pub fn push_fallthrough(&mut self, context: MirFallthroughContext) {
        self.fallthroughs.push(context);
    }

    pub fn pop_fallthrough(&mut self) {
        let _ = self.fallthroughs.pop();
    }

    pub fn current_fallthrough(&self) -> Option<&MirFallthroughContext> {
        self.fallthroughs.last()
    }

    /// Push a handler dispatch context onto the unified handler stack.
    pub fn push_handler(&mut self, context: MirHandlerDispatchContext) {
        self.handlers.push(context);
    }

    /// Pop the top handler dispatch context, returning it for caller cleanup.
    pub fn pop_handler(&mut self) -> MirHandlerDispatchContext {
        self.handlers.pop().expect("handler context should exist")
    }

    /// Return a shared reference to the top handler dispatch context, if any.
    pub fn current_handler(&self) -> Option<&MirHandlerDispatchContext> {
        self.handlers.last()
    }

    /// Return a shared reference to the handler dispatch context at `index`.
    pub fn handler_at(&self, index: usize) -> &MirHandlerDispatchContext {
        &self.handlers[index]
    }

    /// Return a mutable reference to the handler dispatch context at `index`.
    pub fn handler_at_mut(&mut self, index: usize) -> &mut MirHandlerDispatchContext {
        &mut self.handlers[index]
    }

    /// Return the number of active handler dispatch contexts.
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    /// Push a resume continuation context onto the unified resume stack.
    pub fn push_resume(&mut self, context: MirResumeContinuationContext) {
        self.resumes.push(context);
    }

    /// Pop the top resume continuation context.
    pub fn pop_resume(&mut self) {
        let _ = self.resumes.pop();
    }

    /// Return a shared reference to the top resume continuation context, if any.
    pub fn current_resume(&self) -> Option<&MirResumeContinuationContext> {
        self.resumes.last()
    }

    pub fn resolve_loop_index(&self, label: Option<&Identifier>) -> Option<usize> {
        match label {
            Some(label) => {
                let (kind, scope_index) = self.base.resolve_scope(label.as_str())?;
                if kind != ScopeKind::Loop {
                    return None;
                }
                self.loop_index_for_scope(scope_index)
            }
            None => self.loops.len().checked_sub(1),
        }
    }

    pub fn resolve_scope_index(&self, label: &Identifier, expected: ScopeKind) -> Option<usize> {
        let (kind, scope_index) = self.base.resolve_scope(label.as_str())?;
        (kind == expected).then_some(scope_index)
    }

    pub fn loop_at(&self, index: usize) -> &MirLoopContext {
        &self.loops[index]
    }

    pub fn loop_at_mut(&mut self, index: usize) -> &mut MirLoopContext {
        &mut self.loops[index]
    }

    fn loop_index_for_scope(&self, scope_index: usize) -> Option<usize> {
        if scope_index >= self.base.scopes().len() {
            return None;
        }
        if !matches!(self.base.scopes().get(scope_index), Some(ControlFlowScopeKind::Loop(_))) {
            return None;
        }
        let mut loop_index = 0usize;
        for (index, scope) in self.base.scopes().iter().enumerate() {
            if matches!(scope, ControlFlowScopeKind::Loop(_)) {
                if index == scope_index {
                    return Some(loop_index);
                }
                loop_index += 1;
            }
        }
        None
    }
}
