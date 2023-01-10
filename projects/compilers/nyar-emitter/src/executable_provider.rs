//! Driver-side executable query interface.
//!
//! `emitter` should treat language executable structure as private and only
//! consume it through this query surface and the helper types defined here.

use std::collections::BTreeMap;

use nyar::QualifiedName;
use nyar_types::NamePath;
pub type NyarType = nyar::NyarType;
pub type ExecutableValueRef = crate::contracts::ValueRef;
pub type ExecutableBlockRef = crate::contracts::BlockRef;
pub type ExecutableOperand = crate::contracts::Operand;
pub type ExecutableConstant = crate::contracts::Constant;
pub type ExecutableDispatchKind = crate::contracts::DispatchKind;
pub type ExecutableStorageKind = crate::contracts::StorageKind;
pub type ExecutableReceiverPassingKind = crate::contracts::ReceiverPassingKind;
pub type ExecutableInstruction = crate::contracts::Instruction;
pub type ExecutableInstructionKind = crate::contracts::InstructionKind;
pub type ExecutableTerminator = crate::contracts::Terminator;
pub type ExecutableDiagnostic = crate::contracts::Diagnostic;
pub type ExecutableValue = crate::contracts::Value;
pub type ExecutableSuspendPoint = crate::contracts::SuspendPoint;
pub type ExecutableFrameLayout = crate::contracts::FrameLayout;
pub type ExecutableContinuation = crate::contracts::Continuation;
pub type ExecutableCaseChain = crate::contracts::CaseChain;
pub type ExecutableBlock = crate::contracts::Block;
pub type ExecutableSuspendPlan = crate::contracts::SuspendLoweringPlan;
pub type ExecutableFunction = crate::contracts::ExecutableFunction;

/// A driver-side view of function-level executable semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionView {
    pub function: ExecutableFunction,
}

impl FunctionView {
    pub fn symbol(&self) -> &str {
        &self.function.symbol
    }

    pub fn blocks(&self) -> &[ExecutableBlock] {
        &self.function.blocks
    }

    pub fn suspend_points(&self) -> &[ExecutableSuspendPoint] {
        &self.function.suspend_points
    }

    pub fn case_chains(&self) -> &[ExecutableCaseChain] {
        &self.function.case_chains
    }
}

/// Minimal suspend metadata view required by suspend-aware backends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuspendMetadataView {
    pub function_symbol: String,
    pub suspend_points: Vec<ExecutableSuspendPoint>,
    pub frame_layouts: Vec<ExecutableFrameLayout>,
    pub continuations: Vec<ExecutableContinuation>,
    pub case_chains: Vec<ExecutableCaseChain>,
}

impl SuspendMetadataView {
    pub fn from_function(function: &ExecutableFunction) -> Option<Self> {
        let suspend_plan = function.suspend_plan.as_ref()?;
        // `suspend_plan` is canonical; still keep points/layouts as explicit slices for consumers.
        let _ = suspend_plan;
        Some(Self {
            function_symbol: function.symbol.clone(),
            suspend_points: function.suspend_points.clone(),
            frame_layouts: function.frame_layouts.clone(),
            continuations: function.continuations.clone(),
            case_chains: function.case_chains.clone(),
        })
    }
}

/// Provider of executable views for exported operations.
pub trait ExecutableProvider: Send + Sync {
    /// Returns all operations known to the provider (including non-exported helper functions).
    fn operations(&self) -> Vec<QualifiedName>;

    /// Returns the function view for the given exported operation.
    fn get_function(&self, operation: &QualifiedName) -> Option<FunctionView>;

    /// Finds a function by its internal symbol string.
    fn find_by_symbol(&self, symbol: &str) -> Option<FunctionView>;

    /// Returns suspend metadata if the function has suspend semantics.
    fn suspend_metadata(&self, operation: &QualifiedName) -> Option<SuspendMetadataView>;
}

/// Transitional provider backed by the current `mir_functions` map.
#[derive(Debug, Clone)]
pub struct MirFunctionMapProvider {
    functions: BTreeMap<QualifiedName, ExecutableFunction>,
}

impl MirFunctionMapProvider {
    pub fn new(functions: BTreeMap<QualifiedName, ExecutableFunction>) -> Self {
        Self { functions }
    }
}

/// True when `registry_symbol` is exactly `simple` or ends with `::simple` / `.simple`.
pub(crate) fn callee_symbol_ends_with_simple(registry_symbol: &str, simple: &str) -> bool {
    registry_symbol == simple || registry_symbol.ends_with(&format!(".{simple}")) || registry_symbol.ends_with(&format!("::{simple}"))
}

/// Resolve a static `Call` callee symbol to the exact local [`QualifiedName`] operation.
///
/// Aligns Semantic MIR (`SMIR003`) and physical planning (`BPHYS004`): bare names like
/// `answer` must map to a unique registry entry such as `main::answer`.
pub(crate) fn resolve_static_callee_operation(executable: &dyn ExecutableProvider, path: &NamePath) -> Option<QualifiedName> {
    let dotted = path.to_string();
    if let Some(operation) = operation_for_exact_symbol(executable, &dotted) {
        return Some(operation);
    }
    if path.parts().len() > 1 {
        let via_colon = path.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>().join("::");
        if let Some(operation) = operation_for_exact_symbol(executable, &via_colon) {
            return Some(operation);
        }
        let qualified = QualifiedName::new(path.parts().to_vec());
        if executable.get_function(&qualified).is_some() {
            return Some(qualified);
        }
    }
    if path.parts().len() == 1 {
        let simple = path.parts()[0].as_str();
        let matches: Vec<QualifiedName> = executable
            .operations()
            .into_iter()
            .filter(|operation| {
                executable
                    .get_function(operation)
                    .is_some_and(|view| callee_symbol_ends_with_simple(&view.function.symbol, simple))
            })
            .collect();
        if matches.len() == 1 {
            return Some(matches[0].clone());
        }
    }
    None
}

fn operation_for_exact_symbol(executable: &dyn ExecutableProvider, symbol: &str) -> Option<QualifiedName> {
    if executable.find_by_symbol(symbol).is_none() {
        return None;
    }
    executable
        .operations()
        .into_iter()
        .find(|operation| executable.get_function(operation).is_some_and(|view| view.function.symbol == symbol))
}

impl ExecutableProvider for MirFunctionMapProvider {
    fn operations(&self) -> Vec<QualifiedName> {
        self.functions.keys().cloned().collect()
    }

    fn get_function(&self, operation: &QualifiedName) -> Option<FunctionView> {
        self.functions.get(operation).cloned().map(|function| FunctionView { function })
    }

    fn find_by_symbol(&self, symbol: &str) -> Option<FunctionView> {
        self.functions.values().find(|function| function.symbol == symbol).cloned().map(|function| FunctionView { function })
    }

    fn suspend_metadata(&self, operation: &QualifiedName) -> Option<SuspendMetadataView> {
        self.functions.get(operation).and_then(SuspendMetadataView::from_function)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn function(symbol: &str) -> ExecutableFunction {
        ExecutableFunction {
            symbol: symbol.to_string(),
            return_type: NyarType::Unit,
            param_types: Vec::new(),
            value_types: BTreeMap::new(),
            entry: crate::contracts::BlockRef(0),
            values: Vec::new(),
            intrinsic: None,
            suspend_points: Vec::new(),
            frame_layouts: Vec::new(),
            continuations: Vec::new(),
            case_chains: Vec::new(),
            #[allow(deprecated)]
            state_machine: None,
            suspend_plan: None,
            state_machine_lowered: true,
            blocks: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn function_registry_requires_exact_semantic_symbol() {
        let operation = QualifiedName::new(vec![nyar::Identifier::new("module"), nyar::Identifier::new("entry")]);
        let provider = MirFunctionMapProvider::new(BTreeMap::from([(operation, function("module.entry"))]));

        assert!(provider.find_by_symbol("module.entry").is_some());
        assert!(provider.find_by_symbol("entry").is_none());
        assert!(provider.find_by_symbol("other.module.entry").is_none());
        assert!(provider.find_by_symbol("module::entry").is_none());
    }

    #[test]
    fn bare_callee_resolves_to_unique_module_helper() {
        let main_op = QualifiedName::new(vec![nyar::Identifier::new("main"), nyar::Identifier::new("main")]);
        let answer_op = QualifiedName::new(vec![nyar::Identifier::new("main"), nyar::Identifier::new("answer")]);
        let provider = MirFunctionMapProvider::new(BTreeMap::from([
            (main_op, function("main::main")),
            (answer_op, function("main::answer")),
        ]));
        let path = NamePath::new(vec![nyar::Identifier::new("answer")]);
        let expected = QualifiedName::new(vec![nyar::Identifier::new("main"), nyar::Identifier::new("answer")]);
        assert_eq!(resolve_static_callee_operation(&provider, &path), Some(expected));
    }
}
