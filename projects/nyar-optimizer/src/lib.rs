#![doc = include_str!("readme.md")]
#![warn(missing_docs)]

mod egraph;
mod extract;
mod pattern;
mod term;
mod theory;

pub use egraph::{EGraphHost, EGraphStats, StructuredRewrite};
pub use pattern::{TermPattern, parse_pattern};
pub use term::{AlgebraicTerm, parse_term};
pub use theory::{TermRewrite, builtin_core_rewrite_theory, builtin_core_term_rewrites, filter_equations, filter_term_rewrites};

use egraph::EGraphHost as OptimizerEGraph;
use nyar_types::{CapabilityTag, Identifier, QualifiedName};

/// 单个 `Object Algebraic` 语义维度的清单。
///
/// 这里记录的是“这个程序对外暴露了哪些语义视图”，
/// 而不是重新发明一个闭合节点池。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObjectAlgebraicDimension {
    /// 维度名称。
    pub name: Identifier,
    /// 该维度对外提供的语义操作或解释入口。
    pub exported_operations: Vec<QualifiedName>,
    /// 激活该维度所需的能力标签。
    pub required_capabilities: Vec<CapabilityTag>,
    /// 当前维度内操作共同呈现出的引用对象管理提示。
    pub reference_management_hint: Option<ReferenceManagement>,
}

/// `Object Algebraic` 程序边界。
///
/// 当前阶段只保留组合边界和维度清单，
/// 明确拒绝把 `Object Algebraic` 简化为统一节点枚举。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObjectAlgebraicProgram {
    /// 模块名。
    pub module_name: QualifiedName,
    /// 稳定导出符号。
    pub exports: Vec<QualifiedName>,
    /// 已注册的语义维度。
    pub dimensions: Vec<ObjectAlgebraicDimension>,
    /// 可直接进入 `E-Graph` 的结构化项。
    pub structured_terms: Vec<AlgebraicTerm>,
}

impl ObjectAlgebraicProgram {
    /// 注册一个语义维度。
    pub fn register_dimension(&mut self, dimension: ObjectAlgebraicDimension) {
        self.dimensions.push(dimension);
    }
}

/// `Object Algebraic` builder 的最小公共接口。
pub trait ObjectAlgebraicBuilder {
    /// 返回逻辑模块名。
    fn module_name(&self) -> &QualifiedName;

    /// 返回稳定导出符号。
    fn exports(&self) -> &[QualifiedName];

    /// 返回已声明的语义维度。
    fn dimensions(&self) -> &[ObjectAlgebraicDimension];
}

impl ObjectAlgebraicBuilder for ObjectAlgebraicProgram {
    fn module_name(&self) -> &QualifiedName {
        &self.module_name
    }

    fn exports(&self) -> &[QualifiedName] {
        &self.exports
    }

    fn dimensions(&self) -> &[ObjectAlgebraicDimension] {
        &self.dimensions
    }
}

/// `Object Algebraic` 解释器边界。
pub trait ObjectAlgebraicInterpreter {
    /// 返回解释器名。
    fn interpreter_name(&self) -> &Identifier;

    /// 返回解释器支持的能力。
    fn supported_capabilities(&self) -> &[CapabilityTag];
}

/// `E-Graph` 重写所在阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewritePhase {
    /// 规范化阶段。
    Normalize,
    /// 等价饱和阶段。
    Saturate,
    /// 为目标投影做收口的阶段。
    PreProjection,
}

/// 单条等价重写规则。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteRule {
    /// 规则名。
    pub name: Identifier,
    /// 所属阶段。
    pub phase: RewritePhase,
    /// 触发规则所需能力。
    pub required_capabilities: Vec<CapabilityTag>,
    /// 允许投影到的目标家族。
    pub allowed_projection_families: Vec<FutamuraProjectionFamily>,
}

/// 一组重写规则。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RewriteTheory {
    /// 已注册规则。
    pub rules: Vec<RewriteRule>,
    /// 已注册等式。
    pub equations: Vec<RewriteEquation>,
    /// 结构化项级重写。
    pub term_rewrites: Vec<TermRewrite>,
}

impl RewriteTheory {
    /// 注册一条重写规则。
    pub fn register(&mut self, rule: RewriteRule) {
        self.rules.push(rule);
    }

    /// 注册一条等式。
    pub fn equate(&mut self, equation: RewriteEquation) {
        self.equations.push(equation);
    }

    /// 统计某阶段的规则数量。
    pub fn count_in_phase(&self, phase: RewritePhase) -> usize {
        self.rules.iter().filter(|rule| rule.phase == phase).count()
            + self.equations.iter().filter(|equation| equation.phase == phase).count()
            + self.term_rewrites.iter().filter(|rewrite| rewrite.phase == phase).count()
    }

    /// 注册一条结构化项级重写。
    pub fn register_term_rewrite(&mut self, rewrite: TermRewrite) {
        self.term_rewrites.push(rewrite);
    }
}

/// 单条可进入 `E-Graph` 的等式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteEquation {
    /// 等式左侧操作。
    pub left: QualifiedName,
    /// 等式右侧操作。
    pub right: QualifiedName,
    /// 所属阶段。
    pub phase: RewritePhase,
    /// 触发等式所需能力。
    pub required_capabilities: Vec<CapabilityTag>,
}

/// 供片段提交携带的理论 bundle。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TheoryBundle {
    /// 共享理论。
    pub shared: RewriteTheory,
    /// 当前片段私有理论。
    pub fragment: RewriteTheory,
}

impl TheoryBundle {
    /// 合并共享理论与片段私有理论。
    pub fn merged(&self) -> RewriteTheory {
        let mut merged = self.shared.clone();
        for rule in &self.fragment.rules {
            if !merged.rules.contains(rule) {
                merged.rules.push(rule.clone());
            }
        }
        for rewrite in &self.shared.term_rewrites {
            if !merged.term_rewrites.iter().any(|existing| existing.name == rewrite.name) {
                merged.term_rewrites.push(rewrite.clone());
            }
        }
        for equation in &self.fragment.equations {
            if !merged.equations.contains(equation) {
                merged.equations.push(equation.clone());
            }
        }
        for rewrite in &self.fragment.term_rewrites {
            if !merged.term_rewrites.iter().any(|existing| existing.name == rewrite.name) {
                merged.term_rewrites.push(rewrite.clone());
            }
        }
        merged
    }
}

/// `E-Graph` 会话快照。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EGraphSnapshot {
    /// 等价类数量。
    pub equivalence_class_count: usize,
    /// 维度操作数量。
    pub operation_count: usize,
    /// 被真正吸收进 `E-Graph` 的等式数量。
    pub applied_equation_count: usize,
    /// 被应用的模式重写数量。
    pub applied_rewrite_count: usize,
    /// 常量折叠次数。
    pub constant_fold_count: usize,
    /// 饱和迭代轮次。
    pub iteration_count: usize,
    /// 被管理的 `E-Node` 数量。
    pub enode_count: usize,
    /// 是否达到当前理论下的饱和。
    pub saturated: bool,
}

/// `Futamura projection` 目标家族。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FutamuraProjectionFamily {
    /// `futa_clr`
    Clr,
    /// `futa_jvm`
    Jvm,
    /// `futa_wasm`
    Wasm,
    /// `futa_native`
    Native,
    /// `futa_vm`
    NyarVm,
    /// `futa_gpu`
    Gpu,
}

impl FutamuraProjectionFamily {
    /// 返回稳定家族名。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clr => "futa_clr",
            Self::Jvm => "futa_jvm",
            Self::Wasm => "futa_wasm",
            Self::Native => "futa_native",
            Self::NyarVm => "futa_vm",
            Self::Gpu => "futa_gpu",
        }
    }
}

/// 引用语义对象的管理策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReferenceManagement {
    /// 交给精准式托管 `GC` 管理。
    HostGc,
    /// 交给 `Perceus RC` 管理。
    PerceusRc,
}

/// 目标宿主边界。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HostProjectionBoundary {
    /// `.NET` 宿主边界。
    Clr,
    /// `JVM` 宿主边界。
    Jvm,
    /// `WASM + JS glue` 宿主边界。
    WasmJsGlue,
    /// `WASI component model` 宿主边界。
    WasiComponent,
    /// 原生宿主边界。
    Native,
    /// `NyarVM` 宿主边界。
    Vm,
}

/// `Futamura projection` 选择策略。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionPolicy {
    /// 目标投影家族。
    pub family: FutamuraProjectionFamily,
    /// 宿主边界。
    pub host_boundary: HostProjectionBoundary,
    /// 引用对象管理策略。
    pub reference_management: ReferenceManagement,
    /// 是否优先缩小产物体积。
    pub prefer_small_artifacts: bool,
    /// 是否保留显式 effect 边界。
    pub preserve_effect_boundaries: bool,
}

/// 单次目标投影计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionPlan {
    /// 选中的目标家族。
    pub family: FutamuraProjectionFamily,
    /// 选中的宿主边界。
    pub host_boundary: HostProjectionBoundary,
    /// 选中的引用对象管理策略。
    pub reference_management: ReferenceManagement,
    /// 投影后仍需要保留的能力。
    pub preserved_capabilities: Vec<CapabilityTag>,
}

/// 一次优化请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizationRequest {
    /// 输入的 `Object Algebraic` 程序。
    pub program: ObjectAlgebraicProgram,
    /// 当前可用能力。
    pub capabilities: Vec<CapabilityTag>,
    /// 本轮使用的等价理论。
    pub rewrite_theory: RewriteTheory,
    /// 目标投影策略。
    pub projection_policy: ProjectionPolicy,
}

/// 优化结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizationResult {
    /// 选择后的 `Object Algebraic` 程序。
    pub program: ObjectAlgebraicProgram,
    /// 本轮 `E-Graph` 快照。
    pub egraph: EGraphSnapshot,
    /// 目标投影计划。
    pub projection: ProjectionPlan,
    /// 被采用的规则名。
    pub applied_rules: Vec<Identifier>,
}

/// `nyar` 优化会话。
#[derive(Debug, Default)]
pub struct OptimizationSession;

impl OptimizationSession {
    /// 执行 `E-Graph` 饱和、常量折叠与代表元抽取。
    pub fn optimize(&self, request: OptimizationRequest) -> OptimizationResult {
        let theory = merge_with_builtin_core(request.rewrite_theory);
        let applied_rules = theory
            .rules
            .iter()
            .filter(|rule| {
                rule.allowed_projection_families.is_empty() || rule.allowed_projection_families.contains(&request.projection_policy.family)
            })
            .filter(|rule| {
                rule.required_capabilities.iter().all(|capability| request.capabilities.iter().any(|provided| provided == capability))
            })
            .map(|rule| rule.name.clone())
            .collect::<Vec<_>>();

        let mut egraph = OptimizerEGraph::default();
        egraph.ingest_program(&request.program);

        let phases = [RewritePhase::Normalize, RewritePhase::Saturate, RewritePhase::PreProjection];
        let mut stats = egraph.stats().clone();
        for phase in phases {
            let rewrites = filter_term_rewrites(&theory.term_rewrites, phase, &request.capabilities);
            let equations = filter_equations(&theory.equations, phase, &request.capabilities);
            stats = egraph.saturate(&rewrites, &equations);
        }

        let optimized_program = egraph.extract_program(request.program);
        let operation_count = optimized_program.dimensions.iter().map(|dimension| dimension.exported_operations.len()).sum();
        let preserved_capabilities = request
            .capabilities
            .iter()
            .filter(|capability| request.projection_policy.preserve_effect_boundaries || capability.as_str() != "suspend")
            .cloned()
            .collect();

        OptimizationResult {
            program: optimized_program,
            egraph: EGraphSnapshot {
                equivalence_class_count: stats.equivalence_class_count,
                operation_count,
                applied_equation_count: stats.applied_equation_count,
                applied_rewrite_count: stats.applied_rewrite_count,
                constant_fold_count: stats.constant_fold_count,
                iteration_count: stats.iteration_count,
                enode_count: stats.enode_count,
                saturated: stats.saturated,
            },
            projection: ProjectionPlan {
                family: request.projection_policy.family,
                host_boundary: request.projection_policy.host_boundary,
                reference_management: request.projection_policy.reference_management,
                preserved_capabilities,
            },
            applied_rules,
        }
    }
}

fn merge_with_builtin_core(mut theory: RewriteTheory) -> RewriteTheory {
    for rewrite in builtin_core_term_rewrites() {
        if !theory.term_rewrites.iter().any(|existing| existing.name == rewrite.name) {
            theory.term_rewrites.push(rewrite);
        }
    }
    theory
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qualified(parts: &[&str]) -> QualifiedName {
        QualifiedName::new(parts.iter().map(|part| Identifier::new(part)).collect())
    }

    #[test]
    fn constant_fold_addition() {
        let session = OptimizationSession;
        let result = session.optimize(OptimizationRequest {
            program: ObjectAlgebraicProgram {
                module_name: qualified(&["demo"]),
                structured_terms: vec![parse_term("core.add(2, 3)").expect("term")],
                ..Default::default()
            },
            capabilities: Vec::new(),
            rewrite_theory: RewriteTheory::default(),
            projection_policy: ProjectionPolicy {
                family: FutamuraProjectionFamily::Clr,
                host_boundary: HostProjectionBoundary::Clr,
                reference_management: ReferenceManagement::HostGc,
                prefer_small_artifacts: false,
                preserve_effect_boundaries: true,
            },
        });

        assert!(result.egraph.constant_fold_count >= 1);
        assert!(result.egraph.saturated);
        assert_eq!(result.program.structured_terms, vec![AlgebraicTerm::Literal(5)]);
    }

    #[test]
    fn identity_rewrite_eliminates_add_zero() {
        let session = OptimizationSession;
        let x = qualified(&["demo", "x"]);
        let result = session.optimize(OptimizationRequest {
            program: ObjectAlgebraicProgram {
                module_name: qualified(&["demo"]),
                structured_terms: vec![AlgebraicTerm::apply(
                    qualified(&["core", "add"]),
                    vec![AlgebraicTerm::symbol(x.clone()), AlgebraicTerm::literal(0)],
                )],
                ..Default::default()
            },
            capabilities: Vec::new(),
            rewrite_theory: RewriteTheory::default(),
            projection_policy: ProjectionPolicy {
                family: FutamuraProjectionFamily::Clr,
                host_boundary: HostProjectionBoundary::Clr,
                reference_management: ReferenceManagement::HostGc,
                prefer_small_artifacts: false,
                preserve_effect_boundaries: true,
            },
        });

        assert!(result.egraph.applied_rewrite_count >= 1 || result.egraph.constant_fold_count >= 1);
        assert_eq!(result.program.structured_terms, vec![AlgebraicTerm::Symbol(x)]);
    }

    #[test]
    fn nested_constant_fold() {
        let session = OptimizationSession;
        let result = session.optimize(OptimizationRequest {
            program: ObjectAlgebraicProgram {
                module_name: qualified(&["demo"]),
                structured_terms: vec![parse_term("core.add(core.mul(2, 3), 4)").expect("term")],
                ..Default::default()
            },
            capabilities: Vec::new(),
            rewrite_theory: RewriteTheory::default(),
            projection_policy: ProjectionPolicy {
                family: FutamuraProjectionFamily::Clr,
                host_boundary: HostProjectionBoundary::Clr,
                reference_management: ReferenceManagement::HostGc,
                prefer_small_artifacts: false,
                preserve_effect_boundaries: true,
            },
        });

        assert_eq!(result.program.structured_terms, vec![AlgebraicTerm::Literal(10)]);
        assert!(result.egraph.saturated);
    }

    #[test]
    fn flat_equation_union_still_works() {
        let mut rewrite_theory = RewriteTheory::default();
        rewrite_theory.equate(RewriteEquation {
            left: qualified(&["graphic", "dot"]),
            right: qualified(&["graphic", "dot_commutative"]),
            phase: RewritePhase::Saturate,
            required_capabilities: vec![CapabilityTag::new("gpu-shader")],
        });

        let session = OptimizationSession;
        let result = session.optimize(OptimizationRequest {
            program: ObjectAlgebraicProgram {
                module_name: qualified(&["demo"]),
                exports: vec![qualified(&["graphic", "dot"])],
                dimensions: vec![ObjectAlgebraicDimension {
                    name: Identifier::new("graphic"),
                    exported_operations: vec![qualified(&["graphic", "dot"])],
                    required_capabilities: vec![CapabilityTag::new("gpu-shader")],
                    reference_management_hint: None,
                }],
                ..Default::default()
            },
            capabilities: vec![CapabilityTag::new("gpu-shader")],
            rewrite_theory,
            projection_policy: ProjectionPolicy {
                family: FutamuraProjectionFamily::Gpu,
                host_boundary: HostProjectionBoundary::Native,
                reference_management: ReferenceManagement::PerceusRc,
                prefer_small_artifacts: false,
                preserve_effect_boundaries: true,
            },
        });

        assert!(result.egraph.applied_equation_count >= 1);
    }
}
