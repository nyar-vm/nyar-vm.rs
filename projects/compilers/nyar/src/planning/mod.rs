pub mod control_flow;
pub mod rewrite_theory_manifest;

pub use rewrite_theory_manifest::{
    RewriteTheoryEquationEntryV1, RewriteTheoryManifestV1, RewriteTheoryRuleEntryV1, RewriteTheoryTermRewriteEntryV1, builtin_graphic_manifest,
    builtin_neural_manifest,
};

pub use control_flow::{
    ControlFlowPayload, SuspendConsumptionModel, SuspendContinuationArtifact, SuspendDispatchCase, SuspendFunctionArtifact,
    SuspendRuntimeFunctionArtifact, SuspendRuntimePayload, SuspendStateArtifact, SuspendWitnessBinding, suspend_consumption_model,
    suspend_consumption_model_for_lane,
};

use std::collections::BTreeMap;

use nyar_types::RuntimeRequirement;
use nyar_optimizer::{
    AlgebraicTerm, FutamuraProjectionFamily, HostProjectionBoundary, ObjectAlgebraicDimension, ObjectAlgebraicProgram, OptimizationRequest,
    OptimizationResult, OptimizationSession, ProjectionPolicy, ReferenceManagement, RewriteTheory, TheoryBundle,
};
use nyar_types::{
    CapabilityTag, ExternalCallEdge, ExternalImportLink, Identifier, InternalCallEdge, QualifiedName, WitnessCallEdge, WitnessSubmission,
};

use crate::{
    abstractions::{BackendInputKind, BinaryTarget, CanonicalTarget},
    backends::BackendRegistry,
    packaging::TargetLane,
};

/// 规划阶段失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanningError {
    /// Canonical 语义程序或表示计划未能建立。
    SemanticContract {
        /// 当前模块。
        module: String,
        /// 失败阶段。
        stage: nyar_types::CompileStage,
        /// 结构化诊断摘要。
        detail: String,
    },
    /// 没有找到可解释当前片段的后端解释器。
    MissingBackendInterpreter {
        /// 当前片段。
        fragment: Identifier,
        /// 当前投影家族。
        projection_family: FutamuraProjectionFamily,
        /// 当前宿主边界。
        host_boundary: HostProjectionBoundary,
    },
}

/// 进入 `nyar` 规划层的中性输入。
#[derive(Debug, Clone, PartialEq, Eq)]
struct OptimizerInput {
    /// 逻辑模块名。
    pub module_name: QualifiedName,
    /// 目标。
    pub target: CanonicalTarget,
    /// 下游已经闭合好的程序事实。
    pub program_facts: PlannerFacts,
    /// 前端提交的语义片段。
    ///
    /// 这些片段只描述“有哪些可组合的语义视图”和“每个视图自带什么理论”，
    /// 而不是把整个程序压平为一个单体 `IR`。
    pub semantic_fragments: Vec<OptimizerFragment>,
    /// 已经完成前端翻译的 `Object Algebraic` 程序。
    ///
    /// 仅用于等式优化，不承担 callable、入口或导入绑定。
    pub object_algebraic_program: ObjectAlgebraicProgram,
    /// 当前启用的等价理论。
    pub rewrite_theory: RewriteTheory,
    /// 目标投影策略。
    pub projection_policy: ProjectionPolicy,
    /// 面向当前构建请求的后端解释器注册表。
    pub backend_registry: BackendRegistry,
    /// CLR suspend 策略（非 CLR target 时忽略）。
    pub clr_suspend_strategy: crate::backends::clr::ClrSuspendStrategy,
}

/// 单个语义片段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticFragment {
    /// 片段标识。
    pub id: Identifier,
    /// 该片段对外暴露的稳定操作。
    pub exported_operations: Vec<QualifiedName>,
    /// 激活该片段所需的能力标签。
    pub required_capabilities: Vec<CapabilityTag>,
    /// 当前片段的引用对象管理提示。
    pub reference_management_hint: Option<ReferenceManagement>,
    /// 当前片段的可解释入口。
    pub entry_operation: Option<QualifiedName>,
    /// 当前片段内各稳定操作绑定到的外部导入链接。
    pub external_import_links: BTreeMap<QualifiedName, ExternalImportLink>,
    /// 当前片段内已经解析好的外部调用边。
    pub external_call_edges: Vec<ExternalCallEdge>,
    /// 当前片段内已经解析好的内部调用边。
    pub internal_call_edges: Vec<InternalCallEdge>,
    /// 具名 trait 见证表载荷。
    pub witness_tables: Vec<WitnessSubmission>,
    /// 入口 witness 动态调用边。
    pub witness_calls: Vec<WitnessCallEdge>,
    /// 仅属于该片段的等式理论。
    pub rewrite_theory: RewriteTheory,
    /// 显式 `[export]` 的稳定操作 → wasm 公开导出名（如 `two_sum` → `twoSum`）。
    pub wasm_export_names: std::collections::BTreeMap<QualifiedName, String>,
}


/// 从 Canonical 闭包派生的优化器事实视图，不是前端成功合同。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct PlannerFacts {
    module_name: QualifiedName,
    exports: Vec<nyar_analyzer::ExportContract>,
    capabilities: Vec<CapabilityTag>,
    reference_management: Option<ReferenceManagement>,
    runtime_requirements: Vec<RuntimeRequirement>,
}

/// 仅供等式优化的局部名称视图，不携带可执行入口、导入或调用绑定合同。
#[derive(Debug, Clone, PartialEq, Eq)]
struct OptimizerFragment {
    id: Identifier,
    exported_operations: Vec<QualifiedName>,
    required_capabilities: Vec<CapabilityTag>,
    reference_management_hint: Option<ReferenceManagement>,
    rewrite_theory: RewriteTheory,
}

/// 单个分区计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPartition {
    /// 当前分区对应的语义片段。
    pub fragment: Identifier,
    /// 当前片段的解释入口。
    pub entry_operation: Option<nyar_types::ItemInstanceId>,
    /// 选中的后端名。
    pub backend_name: String,
    /// 选中的解释器名。
    pub interpreter: Identifier,
    /// 分区逻辑名。
    pub name: String,
    /// 当前分区对外暴露的操作。
    pub exported_operations: Vec<nyar_types::ItemInstanceId>,
    /// 目标路线。
    pub lane: TargetLane,
    /// 面向的二进制目标。
    pub binary_target: BinaryTarget,
    /// 预期 backend 输入。
    pub input_kind: Option<BackendInputKind>,
    /// CLR suspend 策略（仅 `lane == Clr` 时有效）。
    pub clr_suspend_strategy: crate::backends::clr::ClrSuspendStrategy,
    /// 宿主边界。
    pub host_boundary: HostProjectionBoundary,
    /// 引用对象管理策略。
    pub reference_management: ReferenceManagement,
    /// 本分区要求的能力标签。
    pub capabilities: Vec<CapabilityTag>,
    /// 本分区要求的运行时契约。
    pub runtime_requirements: Vec<RuntimeRequirement>,
}

/// 单个分区已经收口好的后端需求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionBackendRequirement {
    /// 选中的后端名。
    pub backend_name: String,
    /// 选中的解释器名。
    pub interpreter: Identifier,
    /// 当前分区对应的语义片段。
    pub fragment: Identifier,
    /// 当前分区所属路线。
    pub lane: TargetLane,
    /// 当前分区产出的 backend 输入种类。
    pub input_kind: BackendInputKind,
    /// 当前分区面向的目标。
    pub target: BinaryTarget,
    /// 当前分区绑定的宿主边界。
    pub host_boundary: HostProjectionBoundary,
    /// 当前分区采用的引用对象管理策略。
    pub reference_management: ReferenceManagement,
}

/// 单个语义片段的优化视图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FragmentOptimizationView {
    /// 片段标识。
    pub fragment_id: Identifier,
    /// Canonical 合同确定的稳定操作；等式优化不得重绑定函数身份。
    pub canonical_operations: Vec<nyar_types::ItemInstanceId>,
    /// 经抽取后的结构化项（M1 为全局 terms；Phase 2 起可按 operation 过滤）。
    pub structured_terms: Vec<AlgebraicTerm>,
    /// 本轮采用的规则名。
    pub applied_rules: Vec<Identifier>,
    /// 共享理论 + 片段理论合并后的 bundle。
    pub theory_bundle: TheoryBundle,
}

/// 中性的产物分区计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPartitionPlan {
    /// 逻辑模块名。
    pub module_name: QualifiedName,
    /// 目标。
    pub target: CanonicalTarget,
    /// 规划前执行得到的优化结果。
    pub optimization: OptimizationResult,
    /// 按片段暴露的优化视图。
    pub fragment_views: Vec<FragmentOptimizationView>,
    /// 分区列表。
    pub partitions: Vec<ArtifactPartition>,
}

impl ArtifactPartitionPlan {
    /// 从已验证的 Canonical 程序派生分区规划输入。
    ///
    /// 可执行入口与根直接保留 Canonical 身份；optimizer 名称视图只用于等式优化，
    /// 不得从 HIR 或旧前端计划重新收集绑定事实。
    pub fn from_canonical_program(
        program: &nyar_types::CanonicalProgram,
        target: CanonicalTarget,
        rewrite_theory: RewriteTheory,
        projection_policy: ProjectionPolicy,
        backend_registry: BackendRegistry,
        clr_suspend_strategy: crate::backends::clr::ClrSuspendStrategy,
    ) -> Result<Self, PlanningError> {
        program.validate().map_err(|error| PlanningError::SemanticContract {
            module: program.linked.module_name.clone(),
            stage: nyar_types::CompileStage::RepresentationPlan,
            detail: format!("{error:?}"),
        })?;
        let linked = &program.linked;
        let module_name = qualified_module_name(&linked.module_name);
        let (program_facts, semantic_fragments, object_algebraic_program) = canonical_optimizer_input(linked)?;
        Self::plan_from_optimizer_input(OptimizerInput {
            module_name,
            target,
            program_facts,
            semantic_fragments,
            object_algebraic_program,
            rewrite_theory,
            projection_policy,
            backend_registry,
            clr_suspend_strategy,
        }, &linked.fragments)
    }

    /// 基于程序事实生成最小分区计划。
    fn plan_from_optimizer_input(
        input: OptimizerInput,
        fragments: &BTreeMap<Identifier, nyar_types::CanonicalFragment>,
    ) -> Result<Self, PlanningError> {
        let OptimizerInput {
            module_name,
            target,
            program_facts,
            semantic_fragments,
            object_algebraic_program,
            rewrite_theory,
            projection_policy,
            backend_registry,
            clr_suspend_strategy,
        } = input;
        if semantic_fragments.is_empty() {
            return Err(PlanningError::SemanticContract {
                module: module_name.to_string(),
                stage: nyar_types::CompileStage::RepresentationPlan,
                detail: "缺少 Compiler 已验证的 semantic fragment；禁止回退到 ObjectAlgebraicProgram".to_owned(),
            });
        }
        let mut projection_policy = projection_policy;
        projection_policy.reference_management =
            resolve_reference_management(program_facts.reference_management, projection_policy.reference_management, projection_policy.family);
        let program = build_effective_program(&module_name, &program_facts, object_algebraic_program, &semantic_fragments);
        let shared_rewrite_theory = rewrite_theory.clone();
        let rewrite_theory = merge_rewrite_theory(rewrite_theory, &semantic_fragments);
        let optimization = OptimizationSession::default().optimize(OptimizationRequest {
            program,
            capabilities: program_facts.capabilities.clone(),
            rewrite_theory,
            projection_policy,
        });
        let fragment_views = build_fragment_views(&module_name, &shared_rewrite_theory, &semantic_fragments, &optimization, fragments)?;
        let binary_target: BinaryTarget = target.into();
        let partitions = build_partitions(&program_facts, &optimization, binary_target, &backend_registry, clr_suspend_strategy, fragments)?;
        Ok(Self { module_name, target, optimization, fragment_views, partitions })
    }

    /// 从指定分区提取已经完成规划的后端需求。
    pub fn backend_requirement(&self, partition_index: usize) -> Option<PartitionBackendRequirement> {
        let partition = self.partitions.get(partition_index)?;
        Some(PartitionBackendRequirement {
            backend_name: partition.backend_name.clone(),
            interpreter: partition.interpreter.clone(),
            fragment: partition.fragment.clone(),
            lane: partition.lane,
            input_kind: partition.input_kind?,
            target: partition.binary_target.clone(),
            host_boundary: partition.host_boundary,
            reference_management: partition.reference_management,
        })
    }
}

fn qualified_module_name(module_name: &str) -> QualifiedName {
    QualifiedName::new(module_name.split("::").filter(|part| !part.is_empty()).map(Identifier::new).collect())
}

fn canonical_optimizer_input(
    linked: &nyar_types::LinkedSemanticProgram,
) -> Result<(PlannerFacts, Vec<OptimizerFragment>, ObjectAlgebraicProgram), PlanningError> {
    let module_name = qualified_module_name(&linked.module_name);
    let callable_name = |instance: nyar_types::ItemInstanceId| {
        linked.callable_names.get(&instance).cloned().ok_or_else(|| PlanningError::SemanticContract {
            module: linked.module_name.clone(),
            stage: nyar_types::CompileStage::RepresentationPlan,
            detail: format!("Canonical callable `{instance:?}` 缺少限定名称"),
        })
    };
    let mut capabilities = Vec::new();
    let mut fragments = Vec::new();
    for fragment in linked.fragments.values() {
        for capability in &fragment.required_capabilities {
            if !capabilities.contains(capability) { capabilities.push(capability.clone()); }
        }
        let exported_operations = fragment.exported_operations.iter().copied().map(&callable_name).collect::<Result<Vec<_>, _>>()?;
        fragments.push(OptimizerFragment {
            id: fragment.id.clone(), exported_operations, required_capabilities: fragment.required_capabilities.clone(),
            reference_management_hint: None, rewrite_theory: RewriteTheory::default(),
        });
    }
    let exports = linked.exports.iter().map(|(instance, record)| Ok(nyar_analyzer::ExportContract {
        exported_name: Identifier::new(&record.exported_name), local_name: callable_name(*instance)?, partition: None,
    })).collect::<Result<Vec<_>, PlanningError>>()?;
    let program_facts = PlannerFacts {
        module_name: module_name.clone(), exports, capabilities, reference_management: linked.reference_management, runtime_requirements: linked.runtime_requirements.clone(),
    };
    let object_algebraic_program = ObjectAlgebraicProgram {
        module_name: module_name.clone(),
        exports: program_facts.exports.iter().map(|export| export.local_name.clone()).collect(),
        dimensions: fragments.iter().map(|fragment| ObjectAlgebraicDimension {
            name: fragment.id.clone(), exported_operations: fragment.exported_operations.clone(),
            required_capabilities: fragment.required_capabilities.clone(), reference_management_hint: None,
        }).collect(),
        structured_terms: Vec::new(),
    };
    Ok((program_facts, fragments, object_algebraic_program))
}

fn build_effective_program(
    module_name: &QualifiedName,
    program_facts: &PlannerFacts,
    object_algebraic_program: ObjectAlgebraicProgram,
    semantic_fragments: &[OptimizerFragment],
) -> ObjectAlgebraicProgram {
    let mut exports = program_facts.exports.iter().map(|item| item.local_name.clone()).collect::<Vec<_>>();
    for fragment in semantic_fragments {
        for operation in &fragment.exported_operations {
            push_unique_operation(&mut exports, operation.clone());
        }
    }
    ObjectAlgebraicProgram {
        module_name: if object_algebraic_program.module_name.parts().is_empty() { module_name.clone() } else { object_algebraic_program.module_name },
        exports,
        dimensions: semantic_fragments
            .iter()
            .map(|fragment| ObjectAlgebraicDimension {
                name: fragment.id.clone(),
                exported_operations: fragment.exported_operations.clone(),
                required_capabilities: fragment.required_capabilities.clone(),
                reference_management_hint: fragment.reference_management_hint,
            })
            .collect(),
        structured_terms: object_algebraic_program.structured_terms,
    }
}

fn merge_rewrite_theory(mut shared: RewriteTheory, semantic_fragments: &[OptimizerFragment]) -> RewriteTheory {
    for fragment in semantic_fragments {
        for rule in &fragment.rewrite_theory.rules {
            if !shared.rules.contains(rule) {
                shared.rules.push(rule.clone());
            }
        }
        for equation in &fragment.rewrite_theory.equations {
            if !shared.equations.contains(equation) {
                shared.equations.push(equation.clone());
            }
        }
        for rewrite in &fragment.rewrite_theory.term_rewrites {
            if !shared.term_rewrites.iter().any(|existing| existing.name == rewrite.name) {
                shared.term_rewrites.push(rewrite.clone());
            }
        }
    }
    shared
}

fn build_fragment_views(
    module_name: &QualifiedName,
    shared_theory: &RewriteTheory,
    semantic_fragments: &[OptimizerFragment],
    optimization: &OptimizationResult,
    fragments: &BTreeMap<Identifier, nyar_types::CanonicalFragment>,
) -> Result<Vec<FragmentOptimizationView>, PlanningError> {
    optimization
        .program
        .dimensions
        .iter()
        .map(|dimension| {
            let fragment_theory = semantic_fragments
                .iter()
                .find(|fragment| fragment.id == dimension.name)
                .ok_or_else(|| PlanningError::SemanticContract {
                    module: module_name.to_string(),
                    stage: nyar_types::CompileStage::RepresentationPlan,
                    detail: format!("优化维度 `{}` 没有对应的 Compiler semantic fragment", dimension.name),
                })?
                .rewrite_theory
                .clone();
            Ok(FragmentOptimizationView {
                fragment_id: dimension.name.clone(),
                canonical_operations: canonical_fragment(fragments, &dimension.name, module_name)?.exported_operations.clone(),
                structured_terms: optimization.program.structured_terms.clone(),
                applied_rules: optimization.applied_rules.clone(),
                theory_bundle: TheoryBundle { shared: shared_theory.clone(), fragment: fragment_theory },
            })
        })
        .collect()
}

fn push_unique_operation(exports: &mut Vec<QualifiedName>, operation: QualifiedName) {
    if !exports.iter().any(|existing| *existing == operation) {
        exports.push(operation);
    }
}

fn resolve_reference_management(
    preferred: Option<ReferenceManagement>,
    fallback: ReferenceManagement,
    family: FutamuraProjectionFamily,
) -> ReferenceManagement {
    match (family, preferred) {
        (FutamuraProjectionFamily::Native, _) => ReferenceManagement::PerceusRc,
        (_, Some(value)) => value,
        _ => fallback,
    }
}

fn resolve_partition_reference_management(
    program_facts: &PlannerFacts,
    dimension_hint: Option<ReferenceManagement>,
    fallback: ReferenceManagement,
    family: FutamuraProjectionFamily,
) -> ReferenceManagement {
    if family == FutamuraProjectionFamily::Native {
        return ReferenceManagement::PerceusRc;
    }

    dimension_hint.or(program_facts.reference_management).unwrap_or(fallback)
}

fn clr_suspend_strategy_for_lane(
    lane: TargetLane,
    strategy: crate::backends::clr::ClrSuspendStrategy,
) -> crate::backends::clr::ClrSuspendStrategy {
    if lane == TargetLane::Clr { strategy } else { crate::backends::clr::ClrSuspendStrategy::default() }
}

fn build_partitions(
    program_facts: &PlannerFacts,
    optimization: &OptimizationResult,
    binary_target: BinaryTarget,
    backend_registry: &BackendRegistry,
    clr_suspend_strategy: crate::backends::clr::ClrSuspendStrategy,
    fragments: &BTreeMap<Identifier, nyar_types::CanonicalFragment>,
) -> Result<Vec<ArtifactPartition>, PlanningError> {
    optimization
        .program
        .dimensions
        .iter()
        .map(|dimension| {
            let fragment = canonical_fragment(fragments, &dimension.name, &program_facts.module_name)?;
            let capabilities =
                resolve_partition_capabilities(&optimization.projection.preserved_capabilities, &dimension.required_capabilities);
            let reference_management = resolve_partition_reference_management(
                program_facts,
                dimension.reference_management_hint,
                optimization.projection.reference_management,
                optimization.projection.family,
            );
            let backend = backend_registry
                .resolve(
                    &dimension.name,
                    &capabilities,
                    &binary_target,
                    optimization.projection.family,
                    optimization.projection.host_boundary,
                    reference_management,
                )
                .ok_or_else(|| PlanningError::MissingBackendInterpreter {
                    fragment: dimension.name.clone(),
                    projection_family: optimization.projection.family,
                    host_boundary: optimization.projection.host_boundary,
                })?;
            Ok(ArtifactPartition {
                fragment: dimension.name.clone(),
                entry_operation: fragment.entry_operation,
                backend_name: backend.backend_name,
                interpreter: backend.interpreter,
                name: format!("{}::{}", optimization.program.module_name, dimension.name),
                exported_operations: fragment.exported_operations.clone(),
                lane: backend.lane,
                binary_target: binary_target.clone(),
                input_kind: backend.input_kind,
                clr_suspend_strategy: clr_suspend_strategy_for_lane(backend.lane, clr_suspend_strategy),
                host_boundary: backend.host_boundary,
                reference_management: backend.reference_management,
                capabilities: capabilities.clone(),
                runtime_requirements: resolve_partition_runtime_requirements(&program_facts.runtime_requirements, &capabilities),
            })
        })
        .collect()
}

fn canonical_fragment<'program>(
    fragments: &'program BTreeMap<Identifier, nyar_types::CanonicalFragment>,
    identity: &Identifier,
    module_name: &QualifiedName,
) -> Result<&'program nyar_types::CanonicalFragment, PlanningError> {
    fragments.get(identity).ok_or_else(|| PlanningError::SemanticContract {
        module: module_name.to_string(),
        stage: nyar_types::CompileStage::RepresentationPlan,
        detail: format!("优化维度 `{identity}` 缺少 Canonical 片段合同"),
    })
}

fn resolve_partition_capabilities(
    preserved_capabilities: &[CapabilityTag],
    dimension_required_capabilities: &[CapabilityTag],
) -> Vec<CapabilityTag> {
    if dimension_required_capabilities.is_empty() { preserved_capabilities.to_vec() } else { dimension_required_capabilities.to_vec() }
}

fn resolve_partition_runtime_requirements(
    runtime_requirements: &[RuntimeRequirement],
    capabilities: &[CapabilityTag],
) -> Vec<RuntimeRequirement> {
    runtime_requirements
        .iter()
        .filter(|requirement| {
            requirement.key == "reference-management" || capabilities.iter().any(|capability| capability.as_str() == requirement.key)
        })
        .cloned()
        .collect()
}
