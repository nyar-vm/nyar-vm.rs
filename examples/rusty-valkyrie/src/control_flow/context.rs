use std::collections::BTreeMap;

use crate::types::hir::ValkyrieType;

/// Loop scope metadata shared between HIR validation and MIR lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopScopeData {
    /// Optional user label for `break label` / `continue label`.
    pub label: Option<String>,
    /// Whether `break expr` is allowed in this loop (value-context loops).
    pub accepts_break_value: bool,
    /// Expected type for `break expr` when set.
    pub break_value_type: Option<ValkyrieType>,
    /// Temporary flag set while validating a `break expr` value expression.
    /// Allows `await` / `block` / `yield` context checks to defer to type inference during break value validation.
    pub validating_break_value: bool,
}

/// Case chain scope metadata for `case` statement fallthrough.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseChainScopeData {
    /// Optional user label for the case chain scope.
    pub label: Option<String>,
    /// Block index of the next arm, when `fallthrough` should re-enter the next arm probe.
    pub next_arm_target: Option<usize>,
    /// Whether `fallthrough` is permitted in this case chain.
    pub fallthrough_allowed: bool,
    /// Whether validation is currently inside a case arm body (where `fallthrough` is legal).
    pub in_arm_body: bool,
}

/// Generator scope metadata for `yield` / `yield from`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratorScopeData {
    /// Optional user label for the generator scope.
    pub label: Option<String>,
    /// Whether this scope corresponds to an async generator.
    pub is_async_generator: bool,
    /// MIR block indices where yield occurs.
    pub yield_points: Vec<usize>,
    /// Whether `yield` / `yield from` is permitted in this scope.
    pub allow_yield: bool,
}

/// Async scope metadata for `await` / `awake` / `block`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncScopeData {
    /// Optional user label for the async scope.
    pub label: Option<String>,
    /// Whether this scope corresponds to an async function context.
    pub is_async_fn: bool,
    /// MIR block indices where await occurs.
    pub await_points: Vec<usize>,
    /// Whether blocking operations (`block`) are permitted in this scope.
    pub allow_blocking: bool,
}

/// Catch scope metadata for `catch` arms with `raise`/`resume`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatchScopeData {
    /// Optional user label for the catch scope.
    pub label: Option<String>,
    /// Whether `raise` is permitted in this catch scope.
    pub raise_allowed: bool,
    /// Whether `resume` is permitted in this catch scope.
    pub resume_allowed: bool,
    /// Whether validation is currently inside a catch arm body (where `resume` is legal).
    pub in_arm_body: bool,
    /// Nesting depth of this catch scope, derived from the count of active catch scopes.
    pub depth: usize,
}

/// Try-scope metadata for `try?` / `try!` / `try { }` bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TryScopeData {
    /// Whether this try scope is the optional (`try?`) form.
    pub is_optional: bool,
    /// Whether this try scope is the forced (`try!`) form.
    pub is_forced: bool,
    /// Explicit result type for the try scope, when declared.
    pub result_type: Option<ValkyrieType>,
}

/// Kind of control-flow scope for label registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScopeKind {
    /// 循环作用域。
    Loop,
    /// `try` 作用域。
    Try,
    /// `case` 链作用域。
    CaseChain,
    /// 生成器作用域。
    Generator,
    /// 异步作用域。
    Async,
    /// `catch` 作用域。
    Catch,
}

/// One entry on the unified control-flow scope stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlFlowScopeKind {
    /// 循环作用域数据。
    Loop(LoopScopeData),
    /// `try` 作用域数据。
    Try(TryScopeData),
    /// `case` 链作用域数据。
    CaseChain(CaseChainScopeData),
    /// 生成器作用域数据。
    Generator(GeneratorScopeData),
    /// 异步作用域数据。
    Async(AsyncScopeData),
    /// `catch` 作用域数据。
    Catch(CatchScopeData),
}

impl ControlFlowScopeKind {
    /// 返回作用域关联的标签名（若有）。
    pub fn label(&self) -> Option<&str> {
        match self {
            ControlFlowScopeKind::Loop(d) => d.label.as_deref(),
            ControlFlowScopeKind::Try(_) => None,
            ControlFlowScopeKind::CaseChain(d) => d.label.as_deref(),
            ControlFlowScopeKind::Generator(d) => d.label.as_deref(),
            ControlFlowScopeKind::Async(d) => d.label.as_deref(),
            ControlFlowScopeKind::Catch(d) => d.label.as_deref(),
        }
    }
}

/// Unified control-flow context replacing ad-hoc `loop_stack` / `try_scope_stack`.
///
/// Label registry uses a shadowing stack: when the same label name is pushed
/// multiple times (e.g. nested loops with the same label), inner pushes
/// shadow outer ones, and popping restores the previous binding instead of
/// removing the entry entirely. This preserves correct resolution across
/// nested scopes with the same label name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ControlFlowContext {
    scopes: Vec<ControlFlowScopeKind>,
    /// Maps label name to a stack of (scope_kind, index in scopes) entries.
    /// The top of the stack is the innermost active binding for that label.
    label_registry: BTreeMap<String, Vec<(ScopeKind, usize)>>,
}

impl ControlFlowContext {
    /// Returns true when no scopes are active.
    pub fn is_empty(&self) -> bool {
        self.scopes.is_empty()
    }

    /// Clone scope stack for nested lambdas / inherited contexts.
    pub fn clone_scopes(&self) -> Self {
        self.clone()
    }

    /// Clone scopes for a lambda/function boundary: inherits async / generator
    /// capability (so an `async` lambda inside a non-async function is detected),
    /// but **clears the label registry** so `break 'outer` / `continue 'outer`
    /// targeting an enclosing function's loops fails to resolve.
    ///
    /// Break/continue cannot cross function boundaries — a lambda is a fresh
    /// function frame, and its body can only target loops declared within itself.
    pub fn clone_for_function_boundary(&self) -> Self {
        Self { scopes: self.scopes.clone(), label_registry: BTreeMap::new() }
    }

    /// Push a label binding, shadowing any previous binding for the same name.
    fn register_label(&mut self, name: &str, kind: ScopeKind) {
        self.label_registry.entry(name.to_string()).or_default().push((kind, self.scopes.len()));
    }

    /// Pop the topmost binding for `name`. Removes the key entirely when the
    /// stack becomes empty, so `resolve_scope` returns `None` afterwards.
    fn unregister_label(&mut self, name: &str) {
        if let Some(stack) = self.label_registry.get_mut(name) {
            stack.pop();
            if stack.is_empty() {
                self.label_registry.remove(name);
            }
        }
    }

    /// Push a loop scope, registering its label when present.
    pub fn push_loop(&mut self, label: Option<String>, accepts_break_value: bool) {
        if let Some(ref name) = label {
            self.register_label(name, ScopeKind::Loop);
        }
        self.scopes.push(ControlFlowScopeKind::Loop(LoopScopeData {
            label,
            accepts_break_value,
            break_value_type: None,
            validating_break_value: false,
        }));
    }

    /// Pop the innermost loop scope and remove its label from the registry.
    ///
    /// Panics if the innermost scope is not a Loop, since this indicates a
    /// push/pop pairing bug in the caller.
    pub fn pop_loop(&mut self) {
        match self.scopes.pop() {
            Some(ControlFlowScopeKind::Loop(data)) => {
                if let Some(label) = data.label {
                    self.unregister_label(&label);
                }
            }
            other => panic!("pop_loop called when innermost scope is not a Loop: {other:?}"),
        }
    }

    /// Push a try scope.
    pub fn push_try(&mut self, data: TryScopeData) {
        self.scopes.push(ControlFlowScopeKind::Try(data));
    }

    /// Pop the innermost try scope.
    ///
    /// Panics if the innermost scope is not a Try.
    pub fn pop_try(&mut self) {
        match self.scopes.pop() {
            Some(ControlFlowScopeKind::Try(_)) => {}
            other => panic!("pop_try called when innermost scope is not a Try: {other:?}"),
        }
    }

    /// Push a case chain scope (for `case` statement fallthrough).
    pub fn push_case_chain(&mut self, label: Option<String>, next_arm_target: Option<usize>, fallthrough_allowed: bool) {
        if let Some(ref name) = label {
            self.register_label(name, ScopeKind::CaseChain);
        }
        self.scopes.push(ControlFlowScopeKind::CaseChain(CaseChainScopeData {
            label,
            next_arm_target,
            fallthrough_allowed,
            in_arm_body: false,
        }));
    }

    /// Pop the innermost case chain scope.
    ///
    /// Panics if the innermost scope is not a CaseChain.
    pub fn pop_case_chain(&mut self) {
        match self.scopes.pop() {
            Some(ControlFlowScopeKind::CaseChain(data)) => {
                if let Some(label) = data.label {
                    self.unregister_label(&label);
                }
            }
            other => {
                panic!("pop_case_chain called when innermost scope is not a CaseChain: {other:?}")
            }
        }
    }

    /// Push a generator scope (for `yield` / `yield from`).
    pub fn push_generator(&mut self, label: Option<String>, is_async_generator: bool) {
        if let Some(ref name) = label {
            self.register_label(name, ScopeKind::Generator);
        }
        self.scopes.push(ControlFlowScopeKind::Generator(GeneratorScopeData {
            label,
            is_async_generator,
            yield_points: Vec::new(),
            allow_yield: false,
        }));
    }

    /// Pop the innermost generator scope.
    ///
    /// Panics if the innermost scope is not a Generator.
    pub fn pop_generator(&mut self) {
        match self.scopes.pop() {
            Some(ControlFlowScopeKind::Generator(data)) => {
                if let Some(label) = data.label {
                    self.unregister_label(&label);
                }
            }
            other => {
                panic!("pop_generator called when innermost scope is not a Generator: {other:?}")
            }
        }
    }

    /// Push an async scope (for `await` / `awake` / `block`).
    pub fn push_async(&mut self, label: Option<String>, is_async_fn: bool) {
        if let Some(ref name) = label {
            self.register_label(name, ScopeKind::Async);
        }
        self.scopes.push(ControlFlowScopeKind::Async(AsyncScopeData { label, is_async_fn, await_points: Vec::new(), allow_blocking: false }));
    }

    /// Pop the innermost async scope.
    ///
    /// Panics if the innermost scope is not an Async.
    pub fn pop_async(&mut self) {
        match self.scopes.pop() {
            Some(ControlFlowScopeKind::Async(data)) => {
                if let Some(label) = data.label {
                    self.unregister_label(&label);
                }
            }
            other => panic!("pop_async called when innermost scope is not an Async: {other:?}"),
        }
    }

    /// Push a catch scope (for `catch` arms with `raise`/`resume`).
    pub fn push_catch(&mut self, label: Option<String>, raise_allowed: bool, resume_allowed: bool) {
        if let Some(ref name) = label {
            self.register_label(name, ScopeKind::Catch);
        }
        self.scopes.push(ControlFlowScopeKind::Catch(CatchScopeData { label, raise_allowed, resume_allowed, in_arm_body: false, depth: 0 }));
    }

    /// Pop the innermost catch scope.
    ///
    /// Panics if the innermost scope is not a Catch.
    pub fn pop_catch(&mut self) {
        match self.scopes.pop() {
            Some(ControlFlowScopeKind::Catch(data)) => {
                if let Some(label) = data.label {
                    self.unregister_label(&label);
                }
            }
            other => panic!("pop_catch called when innermost scope is not a Catch: {other:?}"),
        }
    }

    /// Resolve a scope by label, returning its (kind, index) pair.
    ///
    /// Returns the innermost active binding for the label, respecting
    /// shadowing by nested scopes with the same label name.
    pub fn resolve_scope(&self, label: &str) -> Option<(ScopeKind, usize)> {
        self.label_registry.get(label).and_then(|stack| stack.last().copied())
    }

    /// Get the innermost generator scope, if any.
    pub fn current_generator_scope(&self) -> Option<&GeneratorScopeData> {
        self.scopes.iter().rev().find_map(|scope| match scope {
            ControlFlowScopeKind::Generator(data) => Some(data),
            _ => None,
        })
    }

    /// Get the innermost async scope, if any.
    pub fn current_async_scope(&self) -> Option<&AsyncScopeData> {
        self.scopes.iter().rev().find_map(|scope| match scope {
            ControlFlowScopeKind::Async(data) => Some(data),
            _ => None,
        })
    }

    /// Get the innermost catch scope, if any.
    pub fn current_catch_scope(&self) -> Option<&CatchScopeData> {
        self.scopes.iter().rev().find_map(|scope| match scope {
            ControlFlowScopeKind::Catch(data) => Some(data),
            _ => None,
        })
    }

    /// Get the innermost case chain scope, if any.
    pub fn current_case_chain_scope(&self) -> Option<&CaseChainScopeData> {
        self.scopes.iter().rev().find_map(|scope| match scope {
            ControlFlowScopeKind::CaseChain(data) => Some(data),
            _ => None,
        })
    }

    /// Mutable innermost generator scope, if any.
    pub fn current_generator_scope_mut(&mut self) -> Option<&mut GeneratorScopeData> {
        self.scopes.iter_mut().rev().find_map(|scope| match scope {
            ControlFlowScopeKind::Generator(data) => Some(data),
            _ => None,
        })
    }

    /// Mutable innermost async scope, if any.
    pub fn current_async_scope_mut(&mut self) -> Option<&mut AsyncScopeData> {
        self.scopes.iter_mut().rev().find_map(|scope| match scope {
            ControlFlowScopeKind::Async(data) => Some(data),
            _ => None,
        })
    }

    /// Mutable innermost catch scope, if any.
    pub fn current_catch_scope_mut(&mut self) -> Option<&mut CatchScopeData> {
        self.scopes.iter_mut().rev().find_map(|scope| match scope {
            ControlFlowScopeKind::Catch(data) => Some(data),
            _ => None,
        })
    }

    /// Mutable innermost case chain scope, if any.
    pub fn current_case_chain_scope_mut(&mut self) -> Option<&mut CaseChainScopeData> {
        self.scopes.iter_mut().rev().find_map(|scope| match scope {
            ControlFlowScopeKind::CaseChain(data) => Some(data),
            _ => None,
        })
    }

    /// Resolve `break` / `break expr` target loop, optionally by label.
    pub fn resolve_loop_mut(&mut self, label: Option<&str>) -> Option<&mut LoopScopeData> {
        if let Some(label) = label {
            let index = self.resolve_scope(label).and_then(|(kind, idx)| (kind == ScopeKind::Loop).then_some(idx))?;
            match self.scopes.get_mut(index) {
                Some(ControlFlowScopeKind::Loop(data)) => Some(data),
                _ => None,
            }
        }
        else {
            self.scopes.iter_mut().rev().find_map(|scope| match scope {
                ControlFlowScopeKind::Loop(data) => Some(data),
                _ => None,
            })
        }
    }

    /// Resolve `continue` target loop, optionally by label.
    pub fn resolve_loop(&self, label: Option<&str>) -> Option<&LoopScopeData> {
        if let Some(label) = label {
            let index = self.resolve_scope(label).and_then(|(kind, idx)| (kind == ScopeKind::Loop).then_some(idx))?;
            match self.scopes.get(index) {
                Some(ControlFlowScopeKind::Loop(data)) => Some(data),
                _ => None,
            }
        }
        else {
            self.scopes.iter().rev().find_map(|scope| match scope {
                ControlFlowScopeKind::Loop(data) => Some(data),
                _ => None,
            })
        }
    }

    /// Returns true when `continue` / `continue label` resolves to an active loop.
    pub fn resolve_continue(&self, label: Option<&str>) -> bool {
        if let Some(label) = label {
            self.resolve_scope(label)
                .is_some_and(|(kind, idx)| kind == ScopeKind::Loop && matches!(self.scopes.get(idx), Some(ControlFlowScopeKind::Loop(_))))
        }
        else {
            self.scopes.iter().any(|scope| matches!(scope, ControlFlowScopeKind::Loop(_)))
        }
    }

    /// Innermost try scope, if any.
    pub fn current_try_scope(&self) -> Option<&TryScopeData> {
        self.scopes.iter().rev().find_map(|scope| match scope {
            ControlFlowScopeKind::Try(data) => Some(data),
            _ => None,
        })
    }

    /// Mutable innermost try scope, if any.
    pub fn current_try_scope_mut(&mut self) -> Option<&mut TryScopeData> {
        self.scopes.iter_mut().rev().find_map(|scope| match scope {
            ControlFlowScopeKind::Try(data) => Some(data),
            _ => None,
        })
    }

    /// Label registry for loop scope resolution.
    ///
    /// Each label name maps to a stack of (kind, scope_index) entries; the
    /// top of the stack is the innermost active binding. Empty stacks are
    /// removed, so any key present in the map has at least one binding.
    pub fn label_registry(&self) -> &BTreeMap<String, Vec<(ScopeKind, usize)>> {
        &self.label_registry
    }

    /// Active scopes from outermost to innermost.
    pub fn scopes(&self) -> &[ControlFlowScopeKind] {
        &self.scopes
    }

    /// True when inside any try scope (for `?` early-exit routing).
    pub fn in_try_scope(&self) -> bool {
        self.current_try_scope().is_some()
    }

    /// Iterate loop scopes from innermost to outermost.
    pub fn loops_innermost_first(&self) -> impl Iterator<Item = &LoopScopeData> {
        self.scopes.iter().rev().filter_map(|scope| match scope {
            ControlFlowScopeKind::Loop(data) => Some(data),
            _ => None,
        })
    }

    /// Returns whether validation is currently inside a case chain arm body.
    pub fn current_case_chain_arm_body(&self) -> bool {
        self.current_case_chain_scope().is_some_and(|scope| scope.in_arm_body)
    }

    /// Returns whether validation is currently inside a catch arm body.
    pub fn current_catch_arm_body(&self) -> bool {
        self.current_catch_scope().is_some_and(|scope| scope.in_arm_body)
    }

    /// Returns the number of active catch scopes.
    pub fn current_catch_depth(&self) -> usize {
        self.scopes.iter().filter(|scope| matches!(scope, ControlFlowScopeKind::Catch(_))).count()
    }

    /// Returns whether the innermost async scope marks an async function context.
    pub fn in_async_scope(&self) -> bool {
        self.current_async_scope().is_some_and(|scope| scope.is_async_fn)
    }

    /// Returns whether any generator scope is currently active.
    pub fn in_generator_scope(&self) -> bool {
        self.current_generator_scope().is_some()
    }

    /// Returns whether `yield` / `yield from` is allowed by any active generator scope.
    pub fn current_allow_yield(&self) -> bool {
        self.scopes.iter().any(|scope| match scope {
            ControlFlowScopeKind::Generator(data) => data.allow_yield,
            _ => false,
        })
    }

    /// Returns whether blocking operations are allowed by the innermost async scope.
    /// Returns `false` when no async scope is active.
    pub fn current_allow_blocking(&self) -> bool {
        self.current_async_scope().is_some_and(|scope| scope.allow_blocking)
    }

    /// Returns whether the innermost loop scope is currently validating a `break expr` value.
    pub fn current_validating_break_value(&self) -> bool {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| match scope {
                ControlFlowScopeKind::Loop(data) => Some(data.validating_break_value),
                _ => None,
            })
            .unwrap_or(false)
    }

    /// Sets the `validating_break_value` flag.
    ///
    /// When `label` is `None`, sets the flag on the innermost active loop
    /// (the implicit target of an unlabelled `break expr`).
    /// When `label` is `Some(name)`, sets the flag on the loop bound to that
    /// label — i.e. the actual target of `break 'label expr`. This ensures
    /// the flag is correctly placed on the resolved break target rather than
    /// on an unrelated innermost loop that happens to be in scope.
    pub fn set_validating_break_value(&mut self, value: bool, label: Option<&str>) {
        let target_index = match label {
            Some(name) => self.resolve_scope(name).and_then(|(kind, idx)| (kind == ScopeKind::Loop).then_some(idx)),
            None => self.scopes.iter().enumerate().rev().find_map(|(idx, scope)| match scope {
                ControlFlowScopeKind::Loop(_) => Some(idx),
                _ => None,
            }),
        };
        if let Some(index) = target_index {
            if let Some(ControlFlowScopeKind::Loop(data)) = self.scopes.get_mut(index) {
                data.validating_break_value = value;
            }
        }
    }
}
