//! Planning-side facade implemented directly on top of `valkyrie` HIR.

use std::collections::{BTreeMap, BTreeSet};

use nyar::{
    ArtifactPartitionPlan, BackendRegistry, CanonicalTarget, CapabilityTag, ClrSuspendStrategy, EntryContract, ExportContract,
    ExternalCallArgument, ExternalCallEdge, ExternalImportLink, FunctionAnalysis, Identifier, ImportContract, InternalCallEdge,
    ObjectAlgebraicDimension, ObjectAlgebraicProgram, PlanningError, PlanningInput, ProgramFacts, ProjectionPolicy, QualifiedName,
    RewriteTheory, RuntimeRequirement, SemanticFragment, WitnessCallEdge, WitnessMethodSlotSubmission, WitnessSubmission,
};

use crate::valkyrie::{
    backend_contract::interop::{function_host_provider_target, function_interop_contract, required_capability, runtime_requirement},
    frontend_contract::gpu_fragment_planning::{append_gpu_dimensions, rewrite_theory_for_fragment},
    mir::ssa::{MirLowerer, MirModule, MirOperand, MirOperation},
    types::{
        NamePath,
        hir::{
            HirBlock, HirExpr, HirExprKind, HirFunction, HirImpl, HirLiteral, HirMatchArm, HirModule, HirStatementKind, HirStringSegment,
            ValkyrieType,
        },
    },
};

pub trait NyarPlanningContract {
    fn program_facts(&self) -> ProgramFacts;
    fn object_algebraic_program(&self) -> ObjectAlgebraicProgram;
    fn neutral_plan(&self) -> FrontendNeutralPlan;

    fn artifact_plan(
        &self,
        target: CanonicalTarget,
        projection_policy: ProjectionPolicy,
        backend_registry: BackendRegistry,
        clr_suspend_strategy: ClrSuspendStrategy,
    ) -> Result<ArtifactPartitionPlan, PlanningError> {
        self.neutral_plan().artifact_plan(target, projection_policy, backend_registry, clr_suspend_strategy)
    }
}

impl NyarPlanningContract for HirModule {
    fn program_facts(&self) -> ProgramFacts {
        hir_module_to_program_facts(self)
    }

    fn object_algebraic_program(&self) -> ObjectAlgebraicProgram {
        hir_module_to_object_algebraic_program(self)
    }

    fn neutral_plan(&self) -> FrontendNeutralPlan {
        hir_module_to_frontend_neutral_plan(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Transitional neutral plan for ArtifactPartition / fragment assembly.
///
/// ADR 0011 wants LinkedSemanticProgram as sole authority; until that wiring
/// lands, seed assembly still needs these side tables on the plan.
pub struct FrontendNeutralPlan {
    pub module_name: QualifiedName,
    pub program_facts: ProgramFacts,
    pub semantic_fragments: Vec<SemanticFragment>,
    pub object_algebraic_program: ObjectAlgebraicProgram,
    pub rewrite_theory: RewriteTheory,
}

impl FrontendNeutralPlan {
    pub fn artifact_plan(
        &self,
        target: CanonicalTarget,
        projection_policy: ProjectionPolicy,
        backend_registry: BackendRegistry,
        clr_suspend_strategy: ClrSuspendStrategy,
    ) -> Result<ArtifactPartitionPlan, PlanningError> {
        ArtifactPartitionPlan::from_input(PlanningInput {
            module_name: self.module_name.clone(),
            target,
            program_facts: self.program_facts.clone(),
            semantic_fragments: self.semantic_fragments.clone(),
            object_algebraic_program: self.object_algebraic_program.clone(),
            rewrite_theory: self.rewrite_theory.clone(),
            projection_policy,
            backend_registry,
            clr_suspend_strategy,
        })
    }
}

pub fn hir_module_to_program_facts(module: &HirModule) -> ProgramFacts {
    let module_name = qualified_name(&module.name);
    let function_interop_contracts = module.functions.iter().map(|function| function_interop_contract(function)).collect::<Vec<_>>();
    let function_host_provider_targets = module.functions.iter().map(|function| function_host_provider_target(function)).collect::<Vec<_>>();
    let exports = module
        .functions
        .iter()
        .filter(|function| function.visibility.access.is_public())
        .map(|function| ExportContract {
            exported_name: function.name.clone(),
            local_name: function_symbol(&module_name, function),
            partition: None,
        })
        .collect();
    let mut functions = module
        .functions
        .iter()
        .zip(function_interop_contracts.iter())
        .zip(function_host_provider_targets.iter())
        .map(|((function, interop_contract), host_provider_for)| FunctionAnalysis {
            symbol: function_symbol(&module_name, function),
            is_external: function.is_abstract,
            can_suspend: crate::valkyrie::hir::control_flow_validation::function_can_suspend(&function.body),
            is_async: crate::valkyrie::hir::control_flow_validation::function_is_async(&function.body),
            uses_host_interop: interop_contract.is_some(),
            external_import_link: interop_contract.clone(),
            host_provider_for: host_provider_for.clone(),
            reference_management_hint: None,
        })
        .collect::<Vec<FunctionAnalysis>>();

    // Semantic-group builds keep only the consumer HIR in FrontendBuildOutput; dependency
    // `[wasm]` / `[clr]` / host_provider surfaces live on imported_semantic_exports.
    // Fold those interop contracts into program facts so assembly can register real
    // external imports (N2) instead of soft-stubbing std.io callees.
    for export in &module.imported_semantic_exports {
        let export_module_name = qualified_name(&export.module);
        for function in &export.functions {
            let interop_contract = function_interop_contract(function);
            let host_provider_for = function_host_provider_target(function);
            if interop_contract.is_none() && host_provider_for.is_none() {
                continue;
            }
            let symbol = function_symbol(&export_module_name, function);
            if functions.iter().any(|existing| existing.symbol == symbol) {
                continue;
            }
            functions.push(FunctionAnalysis {
                symbol,
                is_external: function.is_abstract,
                can_suspend: crate::valkyrie::hir::control_flow_validation::function_can_suspend(&function.body),
                is_async: crate::valkyrie::hir::control_flow_validation::function_is_async(&function.body),
                uses_host_interop: interop_contract.is_some(),
                external_import_link: interop_contract,
                host_provider_for,
                reference_management_hint: None,
            });
        }
    }

    let imports = module
        .imports
        .iter()
        .map(|import| ImportContract {
            path: import.path.clone(),
            local_name: import
                .alias
                .as_ref()
                .map(|alias| QualifiedName::new(vec![alias.clone()]))
                .unwrap_or_else(|| qualified_name(&import.path)),
        })
        .collect();
    let mut capabilities = Vec::new();
    let mut runtime_requirements = Vec::new();
    for function in &functions {
        let Some(interop_contract) = function.external_import_link.as_ref()
        else {
            continue;
        };
        let capability = required_capability(interop_contract);
        if !capabilities.iter().any(|existing| existing == &capability) {
            capabilities.push(capability);
        }

        let requirement = runtime_requirement(interop_contract);
        if !runtime_requirements.iter().any(|existing| existing == &requirement) {
            runtime_requirements.push(requirement);
        }
    }
    if module.functions.iter().any(|function| crate::valkyrie::hir::control_flow_validation::function_needs_suspend_fragment(&function.body)) {
        let suspend_capability = CapabilityTag::new("suspend");
        if !capabilities.iter().any(|existing| existing == &suspend_capability) {
            capabilities.push(suspend_capability);
        }
        let suspend_requirement = RuntimeRequirement { key: "suspend".to_string(), value: "required".to_string() };
        if !runtime_requirements.iter().any(|existing| existing == &suspend_requirement) {
            runtime_requirements.push(suspend_requirement);
        }
    }

    let entries = module
        .functions
        .iter()
        .filter(|function| has_main_annotation(function))
        .map(|function| EntryContract { symbol: function_symbol(&module_name, function), requires_wrapper: false })
        .collect();

    ProgramFacts {
        module_name: module_name.clone(),
        entries,
        imports,
        exports,
        functions,
        type_definitions: Vec::new(),
        capabilities,
        reference_management: None,
        runtime_requirements,
    }
}

pub fn hir_module_to_analysis_artifact(module: &HirModule) -> ProgramFacts {
    hir_module_to_program_facts(module)
}

pub fn hir_module_to_object_algebraic_program(module: &HirModule) -> ObjectAlgebraicProgram {
    let module_name = qualified_name(&module.name);
    let exports = module.functions.iter().map(|function| function_symbol(&module_name, function)).collect::<Vec<_>>();
    let entry_functions = module.functions.iter().filter(|function| has_main_annotation(function)).collect::<Vec<_>>();
    let suspend_capability = vec![CapabilityTag::new("suspend")];
    let sync_operations = module
        .functions
        .iter()
        .filter(|function| !crate::valkyrie::hir::control_flow_validation::function_needs_suspend_fragment(&function.body))
        .map(|function| function_symbol(&module_name, function))
        .collect::<Vec<_>>();
    let suspend_operations = module
        .functions
        .iter()
        .filter(|function| crate::valkyrie::hir::control_flow_validation::function_needs_suspend_fragment(&function.body))
        .map(|function| function_symbol(&module_name, function))
        .collect::<Vec<_>>();

    let mut dimensions = if entry_functions.len() > 1 {
        // 多 entry：每个 `@main` 函数独占一个 execution dimension，并沿
        // internal call graph 做可达性闭包，保证片段内始终包含该入口所需的完整
        // 本地执行闭包，而不是把闭包完整性留给下游装配层或目标侧补洞。
        let program_facts = hir_module_to_program_facts(module);
        let internal_edges = internal_call_edges(module, &program_facts.functions);
        let all_function_symbols = module.functions.iter().map(|function| function_symbol(&module_name, function)).collect::<BTreeSet<_>>();
        entry_functions
            .into_iter()
            .map(|function| {
                let entry_symbol = function_symbol(&module_name, function);
                let reachable = reachable_internal_callee_closure(&entry_symbol, &internal_edges, &all_function_symbols);
                let mut exported_operations = vec![entry_symbol.clone()];
                exported_operations.extend(reachable.into_iter().filter(|symbol| *symbol != entry_symbol));
                ObjectAlgebraicDimension {
                    name: entry_fragment_name(function),
                    exported_operations,
                    required_capabilities: Vec::new(),
                    reference_management_hint: None,
                }
            })
            .collect()
    }
    else if entry_functions.len() == 1 {
        // 单 entry：将同步与 suspend 操作收束进同一个 execution dimension，保持
        // 入口执行闭包在语义层闭合，不把跨片段重新拼接 callable closure 的工作
        // 推给后端。若闭包内含 suspend 语义，则该 execution dimension 带上
        // `suspend` capability，并在后续提交阶段派生对应的 suspend 载荷。
        let has_suspend = !suspend_operations.is_empty();
        vec![ObjectAlgebraicDimension {
            name: if has_suspend { Identifier::new("suspend") } else { Identifier::new("functions") },
            exported_operations: exports.clone(),
            required_capabilities: if has_suspend { vec![CapabilityTag::new("suspend")] } else { Vec::new() },
            reference_management_hint: None,
        }]
    }
    else {
        let mut dimensions = Vec::new();
        if !sync_operations.is_empty() {
            dimensions.push(ObjectAlgebraicDimension {
                name: Identifier::new("functions"),
                exported_operations: sync_operations,
                required_capabilities: Vec::new(),
                reference_management_hint: None,
            });
        }
        if !suspend_operations.is_empty() {
            dimensions.push(ObjectAlgebraicDimension {
                name: Identifier::new("suspend"),
                exported_operations: suspend_operations,
                required_capabilities: suspend_capability,
                reference_management_hint: None,
            });
        }
        if dimensions.is_empty() {
            dimensions.push(ObjectAlgebraicDimension {
                name: Identifier::new("functions"),
                exported_operations: exports.clone(),
                required_capabilities: Vec::new(),
                reference_management_hint: None,
            });
        }
        dimensions
    };
    append_gpu_dimensions(module, &mut dimensions, &module_name);

    ObjectAlgebraicProgram { module_name, exports, dimensions, structured_terms: Vec::new() }
}

/// 计算从 `entry` 出发，沿 `internal_edges` 可达的所有内部被调函数闭包。
///
/// 仅返回在 `all_function_symbols` 集合内的 callee（即本模块内定义的函数），
/// 排除外部导入符号。结果包含间接可达的传递闭包，但不包含 `entry` 本身。
fn reachable_internal_callee_closure(
    entry: &QualifiedName,
    internal_edges: &[InternalCallEdge],
    all_function_symbols: &BTreeSet<QualifiedName>,
) -> BTreeSet<QualifiedName> {
    let mut visited: BTreeSet<QualifiedName> = BTreeSet::new();
    let mut frontier: Vec<QualifiedName> = vec![entry.clone()];
    while let Some(caller) = frontier.pop() {
        if !visited.insert(caller.clone()) {
            continue;
        }
        for edge in internal_edges {
            if edge.caller == caller && all_function_symbols.contains(&edge.callee_symbol) {
                if !visited.contains(&edge.callee_symbol) {
                    frontier.push(edge.callee_symbol.clone());
                }
            }
        }
    }
    visited.remove(entry);
    visited
}

pub fn hir_module_to_frontend_neutral_plan(module: &HirModule) -> FrontendNeutralPlan {
    let program_facts = hir_module_to_program_facts(module);
    let object_algebraic_program = hir_module_to_object_algebraic_program(module);
    let external_call_edges = external_call_edges(module, &program_facts.functions);
    let internal_call_edges = internal_call_edges(module, &program_facts.functions);
    let operation_literal_returns = operation_literal_returns(module);
    let operation_void_returns = operation_void_returns(module);
    let witness_tables = Vec::new();
    let witness_calls = Vec::new();
    let witness_capability = false;
    let semantic_fragments = object_algebraic_program
        .dimensions
        .iter()
        .map(|dimension| {
            let fragment_external_call_edges = external_call_edges_for_operations(&external_call_edges, &dimension.exported_operations);
            let fragment_internal_call_edges = internal_call_edges_for_operations(&internal_call_edges, &dimension.exported_operations);
            let fragment_literal_returns = literal_returns_for_operations(&operation_literal_returns, &dimension.exported_operations);
            let mut required_capabilities = dimension.required_capabilities.clone();
            if witness_capability {
                let tag = CapabilityTag::new("trait-witness");
                if !required_capabilities.iter().any(|cap| cap.as_str() == "trait-witness") {
                    required_capabilities.push(tag);
                }
            }
            let (fragment_witness_tables, fragment_witness_calls) = if witness_capability {
                (witness_tables.clone(), if dimension.name.as_str() == "functions" { witness_calls.clone() } else { Vec::new() })
            }
            else {
                (Vec::new(), Vec::new())
            };
            SemanticFragment {
                id: dimension.name.clone(),
                exported_operations: dimension.exported_operations.clone(),
                required_capabilities,
                reference_management_hint: dimension.reference_management_hint,
                entry_operation: program_facts
                    .entries
                    .iter()
                    .find_map(|entry| {
                        dimension.exported_operations.iter().any(|operation| operation == &entry.symbol).then(|| entry.symbol.clone())
                    })
                    .or_else(|| dimension.exported_operations.first().cloned()),
                external_import_links: external_import_links_for_operations(
                    &program_facts.functions,
                    &dimension.exported_operations,
                    &fragment_external_call_edges,
                ),
                external_call_edges: fragment_external_call_edges,
                internal_call_edges: fragment_internal_call_edges,
                operation_literal_returns: fragment_literal_returns,
                operation_void_returns: operation_void_returns.clone(),
                witness_tables: fragment_witness_tables,
                witness_calls: fragment_witness_calls,
                rewrite_theory: rewrite_theory_for_fragment(dimension.name.as_str()),
            }
        })
        .collect();

    FrontendNeutralPlan {
        module_name: program_facts.module_name.clone(),
        program_facts,
        semantic_fragments,
        object_algebraic_program,
        rewrite_theory: RewriteTheory::default(),
    }
}

fn qualified_name(path: &NamePath) -> QualifiedName {
    QualifiedName::new(path.parts().to_vec())
}

fn function_symbol(_module_name: &QualifiedName, function: &HirFunction) -> QualifiedName {
    if !function.declaring_namespace.parts().is_empty() {
        let mut parts = function.declaring_namespace.parts().to_vec();
        parts.push(function.name.clone());
        return QualifiedName::new(parts);
    }
    let mut parts = _module_name.parts().to_vec();
    if parts.is_empty() {
        parts.push(Identifier::new("app"));
    }
    parts.push(function.name.clone());
    QualifiedName::new(parts)
}

fn has_main_annotation(function: &HirFunction) -> bool {
    function.annotations.iter().any(|attribute| attribute.name.parts().last().is_some_and(|name| name.as_str() == "main"))
}

fn entry_fragment_name(function: &HirFunction) -> Identifier {
    let mut name = String::from("main_");
    for ch in function.name.as_str().chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            name.push(ch);
        }
        else {
            name.push('_');
        }
    }
    Identifier::new(&name)
}

fn external_import_links_for_operations(
    functions: &[FunctionAnalysis],
    operations: &[QualifiedName],
    external_call_edges: &[ExternalCallEdge],
) -> BTreeMap<QualifiedName, ExternalImportLink> {
    let mut links = BTreeMap::new();
    for operation in operations {
        if let Some(link) =
            functions.iter().find(|function| function.symbol == *operation).and_then(|function| function.external_import_link.clone())
        {
            links.insert(operation.clone(), link);
        }
    }
    for edge in external_call_edges {
        if let Some(link) =
            functions.iter().find(|function| function.symbol == edge.callee_symbol).and_then(|function| function.external_import_link.clone())
        {
            links.insert(edge.callee_symbol.clone(), link);
        }
    }
    links
}

fn external_call_edges(module: &HirModule, functions: &[FunctionAnalysis]) -> Vec<ExternalCallEdge> {
    let module_name = qualified_name(&module.name);
    let external_symbols =
        functions.iter().filter_map(|function| function.external_import_link.as_ref().map(|_| function.symbol.clone())).collect::<Vec<_>>();

    module
        .functions
        .iter()
        .flat_map(|function| {
            let caller = function_symbol(&module_name, function);
            let mut edges = Vec::new();
            collect_external_call_edges_from_block(&module_name, &caller, &function.body, &external_symbols, &mut edges);
            edges
        })
        .collect()
}

fn external_call_edges_for_operations(edges: &[ExternalCallEdge], operations: &[QualifiedName]) -> Vec<ExternalCallEdge> {
    edges.iter().filter(|edge| operations.iter().any(|operation| *operation == edge.caller)).cloned().collect()
}

fn internal_call_edges(module: &HirModule, functions: &[FunctionAnalysis]) -> Vec<InternalCallEdge> {
    let module_name = qualified_name(&module.name);
    let external_symbols =
        functions.iter().filter_map(|function| function.external_import_link.as_ref().map(|_| function.symbol.clone())).collect::<Vec<_>>();
    let module_symbols = module.functions.iter().map(|function| function_symbol(&module_name, function)).collect::<Vec<_>>();

    module
        .functions
        .iter()
        .flat_map(|function| {
            let caller = function_symbol(&module_name, function);
            let mut edges = Vec::new();
            collect_internal_call_edges_from_block(&module_name, &caller, &function.body, &module_symbols, &external_symbols, &mut edges);
            edges
        })
        .collect()
}

fn internal_call_edges_for_operations(edges: &[InternalCallEdge], operations: &[QualifiedName]) -> Vec<InternalCallEdge> {
    edges.iter().filter(|edge| operations.iter().any(|operation| *operation == edge.caller)).cloned().collect()
}

fn operation_literal_returns(module: &HirModule) -> BTreeMap<QualifiedName, String> {
    let module_name = qualified_name(&module.name);
    module
        .functions
        .iter()
        .filter_map(|function| {
            let symbol = function_symbol(&module_name, function);
            string_literal_from_function_body(&function.body).map(|literal| (symbol, literal))
        })
        .collect()
}

fn operation_void_returns(module: &HirModule) -> BTreeSet<QualifiedName> {
    let module_name = qualified_name(&module.name);
    module
        .functions
        .iter()
        .filter(|function| matches!(function.return_type, ValkyrieType::Unit))
        .map(|function| function_symbol(&module_name, function))
        .collect()
}

fn literal_returns_for_operations(
    literal_returns: &BTreeMap<QualifiedName, String>,
    operations: &[QualifiedName],
) -> BTreeMap<QualifiedName, String> {
    operations.iter().filter_map(|operation| literal_returns.get(operation).map(|literal| (operation.clone(), literal.clone()))).collect()
}

fn collect_internal_call_edges_from_block(
    module_name: &QualifiedName,
    caller: &QualifiedName,
    block: &HirBlock,
    module_symbols: &[QualifiedName],
    external_symbols: &[QualifiedName],
    edges: &mut Vec<InternalCallEdge>,
) {
    for statement in &block.statements {
        match &statement.kind {
            HirStatementKind::Let { initializer, .. } => {
                if let Some(initializer) = initializer {
                    collect_internal_call_edges_from_expr(module_name, caller, initializer, module_symbols, external_symbols, edges);
                }
            }
            HirStatementKind::Expr(expr) => {
                collect_internal_call_edges_from_expr(module_name, caller, expr, module_symbols, external_symbols, edges);
            }
        }
    }

    if let Some(expr) = &block.expr {
        collect_internal_call_edges_from_expr(module_name, caller, expr, module_symbols, external_symbols, edges);
    }
}

fn collect_internal_call_edges_from_expr(
    module_name: &QualifiedName,
    caller: &QualifiedName,
    expr: &HirExpr,
    module_symbols: &[QualifiedName],
    external_symbols: &[QualifiedName],
    edges: &mut Vec<InternalCallEdge>,
) {
    match &expr.kind {
        HirExprKind::Call { callee, args, resolved } => {
            collect_internal_call_edges_from_expr(module_name, caller, callee, module_symbols, external_symbols, edges);
            for arg in args {
                collect_internal_call_edges_from_expr(module_name, caller, &arg.value, module_symbols, external_symbols, edges);
            }

            let callee_symbol = resolved
                .as_ref()
                .and_then(|resolved| resolve_internal_callee_symbol(caller, &resolved.symbol, module_symbols, external_symbols))
                .or_else(|| resolve_internal_callee_expr(caller, callee, module_symbols, external_symbols));

            if let Some(callee_symbol) = callee_symbol {
                edges.push(InternalCallEdge::new(caller.clone(), callee_symbol));
            }
        }
        HirExprKind::Construct { args, .. } => {
            for arg in args {
                collect_internal_call_edges_from_expr(module_name, caller, arg, module_symbols, external_symbols, edges);
            }
        }
        HirExprKind::FieldInit { value, .. }
        | HirExprKind::ArrayNew { length: value, .. }
        | HirExprKind::FieldAccess { object: value, .. }
        | HirExprKind::Return(Some(value))
        | HirExprKind::Yield(Some(value))
        | HirExprKind::YieldFrom(value)
        | HirExprKind::Await(value)
        | HirExprKind::Awake(value)
        | HirExprKind::BlockOn(value)
        | HirExprKind::Raise(value)
        | HirExprKind::Resume(value)
        | HirExprKind::TryPropagate(value) => {
            collect_internal_call_edges_from_expr(module_name, caller, value, module_symbols, external_symbols, edges);
        }
        HirExprKind::StoreField { object, value, .. } => {
            collect_internal_call_edges_from_expr(module_name, caller, object, module_symbols, external_symbols, edges);
            collect_internal_call_edges_from_expr(module_name, caller, value, module_symbols, external_symbols, edges);
        }
        HirExprKind::GenericApply { callee, .. } => {
            collect_internal_call_edges_from_expr(module_name, caller, callee, module_symbols, external_symbols, edges);
        }
        HirExprKind::Block(block) => {
            collect_internal_call_edges_from_block(module_name, caller, block, module_symbols, external_symbols, edges);
        }
        HirExprKind::TryScope { body, .. } => {
            collect_internal_call_edges_from_block(module_name, caller, body, module_symbols, external_symbols, edges);
        }
        HirExprKind::Lambda { body, .. } => {
            collect_internal_call_edges_from_block(module_name, caller, body, module_symbols, external_symbols, edges);
        }
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            for (_, value) in fields {
                collect_internal_call_edges_from_expr(module_name, caller, value, module_symbols, external_symbols, edges);
            }
            for method in methods {
                collect_internal_call_edges_from_block(module_name, caller, &method.body, module_symbols, external_symbols, edges);
            }
        }
        HirExprKind::If { condition, then_branch, else_branch } | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
            collect_internal_call_edges_from_expr(module_name, caller, condition, module_symbols, external_symbols, edges);
            collect_internal_call_edges_from_block(module_name, caller, then_branch, module_symbols, external_symbols, edges);
            if let Some(else_branch) = else_branch {
                collect_internal_call_edges_from_block(module_name, caller, else_branch, module_symbols, external_symbols, edges);
            }
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            collect_internal_call_edges_from_expr(module_name, caller, scrutinee, module_symbols, external_symbols, edges);
            collect_internal_call_edges_from_arms_internal(module_name, caller, arms, module_symbols, external_symbols, edges);
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            if let Some(iterator) = iterator {
                collect_internal_call_edges_from_expr(module_name, caller, iterator, module_symbols, external_symbols, edges);
            }
            if let Some(condition) = condition {
                collect_internal_call_edges_from_expr(module_name, caller, condition, module_symbols, external_symbols, edges);
            }
            collect_internal_call_edges_from_block(module_name, caller, body, module_symbols, external_symbols, edges);
        }
        HirExprKind::Assign { value, .. } => {
            collect_internal_call_edges_from_expr(module_name, caller, value, module_symbols, external_symbols, edges);
        }
        HirExprKind::Break { expr, .. } => {
            if let Some(expr) = expr {
                collect_internal_call_edges_from_expr(module_name, caller, expr, module_symbols, external_symbols, edges);
            }
        }
        HirExprKind::Catch { expr, arms } => {
            collect_internal_call_edges_from_expr(module_name, caller, expr, module_symbols, external_symbols, edges);
            collect_internal_call_edges_from_arms_internal(module_name, caller, arms, module_symbols, external_symbols, edges);
        }
        HirExprKind::With { base, updates } => {
            collect_internal_call_edges_from_expr(module_name, caller, base, module_symbols, external_symbols, edges);
            for (_, update) in updates {
                collect_internal_call_edges_from_expr(module_name, caller, update, module_symbols, external_symbols, edges);
            }
        }
        HirExprKind::SuperCall { args, .. } => {
            for arg in args {
                collect_internal_call_edges_from_expr(module_name, caller, arg, module_symbols, external_symbols, edges);
            }
        }
        HirExprKind::Literal(_)
        | HirExprKind::Variable(_)
        | HirExprKind::Path(_)
        | HirExprKind::ArrayLiteral { .. }
        | HirExprKind::Return(None)
        | HirExprKind::Continue { .. }
        | HirExprKind::Yield(None)
        | HirExprKind::Fallthrough => {}
    }
}

fn collect_internal_call_edges_from_arms_internal(
    module_name: &QualifiedName,
    caller: &QualifiedName,
    arms: &[HirMatchArm],
    module_symbols: &[QualifiedName],
    external_symbols: &[QualifiedName],
    edges: &mut Vec<InternalCallEdge>,
) {
    for arm in arms {
        if let Some(guard) = &arm.guard {
            collect_internal_call_edges_from_expr(module_name, caller, guard, module_symbols, external_symbols, edges);
        }
        collect_internal_call_edges_from_expr(module_name, caller, &arm.body, module_symbols, external_symbols, edges);
    }
}

fn resolve_internal_callee_symbol(
    caller: &QualifiedName,
    resolved_symbol: &NamePath,
    module_symbols: &[QualifiedName],
    external_symbols: &[QualifiedName],
) -> Option<QualifiedName> {
    let direct = qualified_name(resolved_symbol);
    if external_symbols.iter().any(|symbol| *symbol == direct) {
        return None;
    }
    if module_symbols.iter().any(|symbol| *symbol == direct) {
        return Some(direct);
    }

    if resolved_symbol.parts().len() != 1 {
        return None;
    }

    let mut qualified_parts = if caller.parts().len() > 1 { caller.parts()[..caller.parts().len() - 1].to_vec() } else { Vec::new() };
    if qualified_parts.is_empty() {
        qualified_parts.push(Identifier::new("app"));
    }
    qualified_parts.extend_from_slice(resolved_symbol.parts());
    let qualified = QualifiedName::new(qualified_parts);
    if external_symbols.iter().any(|symbol| *symbol == qualified) {
        return None;
    }
    module_symbols.iter().any(|symbol| *symbol == qualified).then_some(qualified)
}

fn resolve_internal_callee_expr(
    caller: &QualifiedName,
    callee: &HirExpr,
    module_symbols: &[QualifiedName],
    external_symbols: &[QualifiedName],
) -> Option<QualifiedName> {
    match &callee.kind {
        HirExprKind::Variable(identifier) => {
            resolve_internal_callee_symbol(caller, &NamePath::new(vec![identifier.name.clone()]), module_symbols, external_symbols)
        }
        HirExprKind::Path(path) => resolve_internal_callee_symbol(caller, path, module_symbols, external_symbols),
        _ => None,
    }
}

fn collect_external_call_edges_from_block(
    module_name: &QualifiedName,
    caller: &QualifiedName,
    block: &HirBlock,
    external_symbols: &[QualifiedName],
    edges: &mut Vec<ExternalCallEdge>,
) {
    for statement in &block.statements {
        match &statement.kind {
            HirStatementKind::Let { initializer, .. } => {
                if let Some(initializer) = initializer {
                    collect_external_call_edges_from_expr(module_name, caller, initializer, external_symbols, edges);
                }
            }
            HirStatementKind::Expr(expr) => collect_external_call_edges_from_expr(module_name, caller, expr, external_symbols, edges),
        }
    }

    if let Some(expr) = &block.expr {
        collect_external_call_edges_from_expr(module_name, caller, expr, external_symbols, edges);
    }
}

fn collect_external_call_edges_from_expr(
    module_name: &QualifiedName,
    caller: &QualifiedName,
    expr: &HirExpr,
    external_symbols: &[QualifiedName],
    edges: &mut Vec<ExternalCallEdge>,
) {
    match &expr.kind {
        HirExprKind::Call { callee, args, resolved } => {
            collect_external_call_edges_from_expr(module_name, caller, callee, external_symbols, edges);
            for arg in args {
                collect_external_call_edges_from_expr(module_name, caller, &arg.value, external_symbols, edges);
            }

            let callee_symbol = resolved
                .as_ref()
                .and_then(|resolved| resolve_external_callee_symbol(caller, &resolved.symbol, external_symbols))
                .or_else(|| resolve_external_callee_expr(caller, callee, external_symbols));

            if let Some(callee_symbol) = callee_symbol {
                edges.push(ExternalCallEdge::new(
                    caller.clone(),
                    callee_symbol,
                    args.iter().filter_map(|arg| external_call_argument(&arg.value)).collect(),
                ));
            }
        }
        HirExprKind::Construct { args, .. } => {
            for arg in args {
                collect_external_call_edges_from_expr(module_name, caller, arg, external_symbols, edges);
            }
        }
        HirExprKind::FieldInit { value, .. }
        | HirExprKind::ArrayNew { length: value, .. }
        | HirExprKind::FieldAccess { object: value, .. }
        | HirExprKind::Return(Some(value))
        | HirExprKind::Yield(Some(value))
        | HirExprKind::YieldFrom(value)
        | HirExprKind::Await(value)
        | HirExprKind::Awake(value)
        | HirExprKind::BlockOn(value)
        | HirExprKind::Raise(value)
        | HirExprKind::Resume(value)
        | HirExprKind::TryPropagate(value) => collect_external_call_edges_from_expr(module_name, caller, value, external_symbols, edges),
        HirExprKind::StoreField { object, value, .. } => {
            collect_external_call_edges_from_expr(module_name, caller, object, external_symbols, edges);
            collect_external_call_edges_from_expr(module_name, caller, value, external_symbols, edges);
        }
        HirExprKind::GenericApply { callee, .. } => collect_external_call_edges_from_expr(module_name, caller, callee, external_symbols, edges),
        HirExprKind::Block(block) => collect_external_call_edges_from_block(module_name, caller, block, external_symbols, edges),
        HirExprKind::TryScope { body, .. } => collect_external_call_edges_from_block(module_name, caller, body, external_symbols, edges),
        HirExprKind::Lambda { body, .. } => collect_external_call_edges_from_block(module_name, caller, body, external_symbols, edges),
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            for (_, value) in fields {
                collect_external_call_edges_from_expr(module_name, caller, value, external_symbols, edges);
            }
            for method in methods {
                collect_external_call_edges_from_block(module_name, caller, &method.body, external_symbols, edges);
            }
        }
        HirExprKind::If { condition, then_branch, else_branch } | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
            collect_external_call_edges_from_expr(module_name, caller, condition, external_symbols, edges);
            collect_external_call_edges_from_block(module_name, caller, then_branch, external_symbols, edges);
            if let Some(else_branch) = else_branch {
                collect_external_call_edges_from_block(module_name, caller, else_branch, external_symbols, edges);
            }
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            collect_external_call_edges_from_expr(module_name, caller, scrutinee, external_symbols, edges);
            collect_external_call_edges_from_arms(module_name, caller, arms, external_symbols, edges);
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            if let Some(iterator) = iterator {
                collect_external_call_edges_from_expr(module_name, caller, iterator, external_symbols, edges);
            }
            if let Some(condition) = condition {
                collect_external_call_edges_from_expr(module_name, caller, condition, external_symbols, edges);
            }
            collect_external_call_edges_from_block(module_name, caller, body, external_symbols, edges);
        }
        HirExprKind::Assign { value, .. } => collect_external_call_edges_from_expr(module_name, caller, value, external_symbols, edges),
        HirExprKind::Break { expr, .. } => {
            if let Some(expr) = expr {
                collect_external_call_edges_from_expr(module_name, caller, expr, external_symbols, edges);
            }
        }
        HirExprKind::Catch { expr, arms } => {
            collect_external_call_edges_from_expr(module_name, caller, expr, external_symbols, edges);
            collect_external_call_edges_from_arms(module_name, caller, arms, external_symbols, edges);
        }
        HirExprKind::With { base, updates } => {
            collect_external_call_edges_from_expr(module_name, caller, base, external_symbols, edges);
            for (_, update) in updates {
                collect_external_call_edges_from_expr(module_name, caller, update, external_symbols, edges);
            }
        }
        HirExprKind::SuperCall { args, .. } => {
            for arg in args {
                collect_external_call_edges_from_expr(module_name, caller, arg, external_symbols, edges);
            }
        }
        HirExprKind::Literal(_)
        | HirExprKind::Variable(_)
        | HirExprKind::Path(_)
        | HirExprKind::ArrayLiteral { .. }
        | HirExprKind::Return(None)
        | HirExprKind::Continue { .. }
        | HirExprKind::Yield(None)
        | HirExprKind::Fallthrough => {}
    }
}

fn collect_external_call_edges_from_arms(
    module_name: &QualifiedName,
    caller: &QualifiedName,
    arms: &[HirMatchArm],
    external_symbols: &[QualifiedName],
    edges: &mut Vec<ExternalCallEdge>,
) {
    for arm in arms {
        if let Some(guard) = &arm.guard {
            collect_external_call_edges_from_expr(module_name, caller, guard, external_symbols, edges);
        }
        collect_external_call_edges_from_expr(module_name, caller, &arm.body, external_symbols, edges);
    }
}

fn resolve_external_callee_symbol(
    caller: &QualifiedName,
    resolved_symbol: &NamePath,
    external_symbols: &[QualifiedName],
) -> Option<QualifiedName> {
    let direct = qualified_name(resolved_symbol);
    if external_symbols.iter().any(|symbol| *symbol == direct) {
        return Some(direct);
    }

    if resolved_symbol.parts().len() != 1 {
        return None;
    }

    let simple = resolved_symbol.parts().last()?;
    if caller.parts().len() > 1 {
        let mut qualified_parts = caller.parts()[..caller.parts().len() - 1].to_vec();
        qualified_parts.push(simple.clone());
        let qualified = QualifiedName::new(qualified_parts);
        if external_symbols.iter().any(|symbol| *symbol == qualified) {
            return Some(qualified);
        }
    }

    external_symbols.iter().find(|symbol| symbol.parts().last() == Some(simple)).cloned()
}

fn resolve_external_callee_expr(caller: &QualifiedName, callee: &HirExpr, external_symbols: &[QualifiedName]) -> Option<QualifiedName> {
    match &callee.kind {
        HirExprKind::Variable(identifier) => {
            resolve_external_callee_symbol(caller, &NamePath::new(vec![identifier.name.clone()]), external_symbols)
        }
        HirExprKind::Path(path) => resolve_external_callee_symbol(caller, path, external_symbols),
        _ => None,
    }
}

fn external_call_argument(expr: &HirExpr) -> Option<ExternalCallArgument> {
    string_literal_from_expr(expr).map(ExternalCallArgument::StringLiteral)
}

fn string_literal_from_expr(expr: &HirExpr) -> Option<String> {
    let HirExprKind::Literal(HirLiteral::String(literal)) = &expr.kind
    else {
        return None;
    };

    let mut rendered = String::new();
    for segment in &literal.segments {
        let HirStringSegment::Text(text) = segment
        else {
            return None;
        };
        rendered.push_str(text);
    }
    Some(rendered)
}

fn string_literal_from_function_body(body: &HirBlock) -> Option<String> {
    if let Some(expr) = &body.expr {
        if let Some(literal) = string_literal_from_return_expr(expr) {
            return Some(literal);
        }
    }
    for statement in &body.statements {
        let HirStatementKind::Expr(expr) = &statement.kind
        else {
            continue;
        };
        if let Some(literal) = string_literal_from_return_expr(expr) {
            return Some(literal);
        }
    }
    None
}

fn string_literal_from_return_expr(expr: &HirExpr) -> Option<String> {
    match &expr.kind {
        HirExprKind::Return(Some(value)) => string_literal_from_expr(value),
        _ => None,
    }
}

fn sanitize_witness_symbol(value: &str) -> String {
    value.chars().map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' }).collect()
}

fn entry_witness_calls_fallback(tables: &[WitnessSubmission]) -> Vec<WitnessCallEdge> {
    tables
        .iter()
        .filter(|table| table.trait_name == "Animal" && !table.result_literal.is_empty())
        .map(|table| WitnessCallEdge {
            trait_name: table.trait_name.clone(),
            type_name: table.type_name.clone(),
            method_index: 0,
            print_result: true,
        })
        .collect()
}

fn push_unique_witness_call(calls: &mut Vec<WitnessCallEdge>, edge: WitnessCallEdge) {
    if calls.iter().any(|existing| {
        existing.trait_name == edge.trait_name && existing.type_name == edge.type_name && existing.method_index == edge.method_index
    }) {
        return;
    }
    calls.push(edge);
}

fn callee_method_name(callee: &MirOperand) -> Option<&str> {
    match callee {
        MirOperand::Symbol(path) => path.parts().last().map(|name| name.as_str()),
        _ => None,
    }
}

fn resolve_receiver_type<'a>(
    witness: Option<&MirOperand>,
    receiver: Option<&MirOperand>,
    value_types: &'a std::collections::BTreeMap<crate::valkyrie::mir::ssa::MirValueRef, ValkyrieType>,
) -> Option<&'a ValkyrieType> {
    let operand = witness.or(receiver)?;
    match operand {
        MirOperand::Value(value_ref) => value_types.get(value_ref),
        _ => None,
    }
}

fn resolve_witness_call_edge(method_name: &str, receiver_ty: Option<&ValkyrieType>, tables: &[WitnessSubmission]) -> Option<WitnessCallEdge> {
    let type_name = receiver_ty.and_then(concrete_type_name).or_else(|| {
        tables.iter().find(|table| table.methods.iter().any(|slot| slot.method_name == method_name)).map(|table| table.type_name.as_str())
    })?;

    let table = tables.iter().find(|table| table.type_name == type_name)?;
    let method_index = table.methods.iter().find(|slot| slot.method_name == method_name).map(|slot| slot.method_index).unwrap_or(0);

    Some(WitnessCallEdge {
        trait_name: table.trait_name.clone(),
        type_name: table.type_name.clone(),
        method_index,
        print_result: witness_return_should_print(table, method_index),
    })
}

fn concrete_type_name(ty: &ValkyrieType) -> Option<&str> {
    match ty {
        ValkyrieType::Named(name) => Some(name.as_str()),
        ValkyrieType::Apply(base, _) => concrete_type_name(base),
        ValkyrieType::TraitObject(object) => Some(object.trait_path.as_str()),
        _ => None,
    }
}

fn witness_return_should_print(table: &WitnessSubmission, method_index: u32) -> bool {
    if method_index == 0 && !table.result_literal.is_empty() {
        return true;
    }
    table.methods.iter().find(|slot| slot.method_index == method_index).is_some_and(|_| !table.result_literal.is_empty())
}

fn impl_nominal_type_name(ty: &ValkyrieType) -> Option<&str> {
    match ty {
        ValkyrieType::Named(name) => Some(name.as_str()),
        ValkyrieType::Apply(base, _) => impl_nominal_type_name(base),
        _ => None,
    }
}

fn witness_submission_from_impl(trait_impl: &HirImpl) -> Option<WitnessSubmission> {
    let trait_name = trait_impl.trait_path.as_ref()?.name().as_str().to_string();
    let type_name = impl_nominal_type_name(&trait_impl.target)?.to_string();
    if trait_name.is_empty() || type_name.is_empty() || trait_impl.methods.is_empty() {
        return None;
    }

    let methods = trait_impl
        .methods
        .iter()
        .enumerate()
        .map(|(index, method)| WitnessMethodSlotSubmission {
            method_name: method.name.as_str().to_string(),
            // This is the canonical MIR operation implemented by the impl block.
            // Witness dispatch carries the resolved operation metadata; it must
            // never manufacture a backend-only `witness_*` symbol.
            impl_symbol: format!("{}.{}", type_name, method.name),
            method_index: u32::try_from(index).unwrap_or(0),
        })
        .collect::<Vec<_>>();

    let result_literal = trait_impl.methods.first().and_then(|method| string_literal_from_function_body(&method.body)).unwrap_or_default();

    Some(WitnessSubmission {
        type_name: type_name.clone(),
        trait_name: trait_name.clone(),
        table_label: format!("witness_table_{}_{}", sanitize_witness_symbol(&type_name), sanitize_witness_symbol(&trait_name)),
        fat_ptr_label: format!("witness_fat_{}_{}", sanitize_witness_symbol(&type_name), sanitize_witness_symbol(&trait_name)),
        methods,
        result_literal,
    })
}
