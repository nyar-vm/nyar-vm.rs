//! 规范程序成功类型与编译管线阶段。
//!
//! 失败侧使用**结构化诊断**（共享合同的一族诊断类型），
//! 而不是名叫 `StructuredDiagnostics` 的单一结构体。

use crate::semantic_ids::{EvidenceId, ItemId, ItemInstanceId, MirValueId, NominalInstanceId, SubstitutionId, TypeId, TypeInstanceId};
use std::collections::BTreeMap;

/// One structured diagnostic record (minimum contract fields).
///
/// Concrete compile stages may wrap or extend this; the category is plural.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticRecord {
    /// Stable machine code (e.g. `SMIR006`).
    pub code: String,
    /// Severity label (`error` / `warning` / …).
    pub severity: String,
    /// Pipeline stage that produced the diagnostic.
    pub stage: CompileStage,
    /// Owning module / package symbol when known.
    pub module: String,
    /// Human message (not used for semantic decisions).
    pub message: String,
    /// Deterministic sort key.
    pub stable_sort_key: String,
}

/// A non-empty structured diagnostics payload (category, not a singleton type name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredDiagnosticSet {
    /// Ordered diagnostic records.
    pub records: Vec<DiagnosticRecord>,
}

impl StructuredDiagnosticSet {
    /// Construct from one or more records. Empty sets are not allowed for `Err`.
    pub fn from_records(records: Vec<DiagnosticRecord>) -> Option<Self> {
        if records.is_empty() { None } else { Some(Self { records }) }
    }
}

/// Stages of the one-way compile / analysis / processing stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompileStage {
    /// Package AST.
    Ast,
    /// HIR elaboration / overload / evidence solving (may use work types).
    Hir,
    /// Package semantic MIR + SPI (M1).
    SemanticMir,
    /// Cross-package link with selected std adaptors.
    LinkTime,
    /// Validated Semantic MIR (M2).
    ValidateMir,
    /// Sparse representation / layout planning.
    RepresentationPlan,
    /// Target-private plan.
    BackendPrivatePlan,
    /// Artifact emit.
    Emit,
}

/// Result alias for pipeline stages: success value or structured diagnostics category.
pub type StageResult<T> = Result<T, StructuredDiagnosticSet>;

/// 完成 adaptor 选择与跨包闭包后的链接程序。
///
/// 这是进入已校验 Semantic MIR 的**成功**类型——不是并行的
/// `FrontendNeutralPlan` / `FragmentSubmission` 权威。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkedSemanticProgram {
    /// Module / package identity key.
    pub module_name: String,
    /// Closed item instances (bodies + evidence reachable).
    pub item_instances: BTreeMap<ItemInstanceId, ItemInstanceRecord>,
    /// Closed nominal ADT instances.
    pub nominal_instances: BTreeMap<NominalInstanceId, NominalInstanceRecord>,
    /// Selected evidence bindings.
    pub evidence: BTreeMap<EvidenceId, EvidenceRecord>,
    /// Semantic type table.
    pub types: BTreeMap<TypeId, TypeRecord>,
}

/// Placeholder item instance row (filled by linker / adaptor selection).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemInstanceRecord {
    /// 已声明的 callable identity。
    pub declaration: ItemId,
    /// 完成泛型代入后的 substitution identity。
    pub substitution: SubstitutionId,
}

/// Placeholder nominal instance row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NominalInstanceRecord {
    /// 名义类型声明身份。
    pub declaration: TypeId,
    /// 完成代入后的类型实例身份。
    pub substitution: SubstitutionId,
}

/// Placeholder evidence row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRecord {
    /// trait 或 adaptor 声明身份。
    pub trait_id: ItemId,
    /// 实现方的具体类型实例身份。
    pub implementing_type: TypeInstanceId,
}

/// Placeholder type table row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRecord {
    /// 类型表中的声明身份。
    pub declaration: TypeId,
}

/// Semantic call edge whose callee identity was fixed before representation planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalCall {
    /// 已实例化的被调用项。
    pub callee: ItemInstanceId,
    /// 已定义的 SSA 实参。
    pub arguments: Vec<MirValueId>,
    /// 调用结果值；无结果调用必须显式为 `None`。
    pub result: Option<MirValueId>,
}

/// One validated function body in Semantic MIR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalFunction {
    /// 此函数对应的已实例化项。
    pub instance: ItemInstanceId,
    /// 入口参数的 SSA 值与类型身份。
    pub parameters: Vec<(MirValueId, TypeId)>,
    /// 返回类型身份。
    pub return_type: TypeId,
    /// 已解析调用边；后端不得重新解析 callee。
    pub calls: Vec<CanonicalCall>,
}

/// Validated Semantic MIR package owned by the success path (no embedded diagnostics).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonicalSemanticMir {
    /// Owning linked program identity.
    pub module_name: String,
    /// 以稳定实例身份索引的完整函数合同。
    pub functions: BTreeMap<ItemInstanceId, CanonicalFunction>,
}

/// Canonical MIR 合同失败的确定性原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalMirError {
    /// 表键与函数内部实例身份不一致。
    FunctionKeyMismatch { key: ItemInstanceId, instance: ItemInstanceId },
    /// Semantic MIR 函数自身不在链接闭包中。
    UnknownFunction { function: ItemInstanceId },
    /// 函数引用了未链接的 callee。
    UnknownCallee { function: ItemInstanceId, callee: ItemInstanceId },
    /// 类型身份未进入 canonical type table。
    UnknownType { function: ItemInstanceId, ty: TypeId },
    /// SSA 值在定义前被使用。
    UseBeforeDefinition { function: ItemInstanceId, value: MirValueId },
    /// 一个 SSA 值被重复定义。
    DuplicateDefinition { function: ItemInstanceId, value: MirValueId },
}

impl CanonicalSemanticMir {
    /// 在进入 RepresentationPlan 前验证 stable-ID、链接和 SSA 合同。
    pub fn validate(&self, linked: &LinkedSemanticProgram) -> Result<(), CanonicalMirError> {
        for (key, function) in &self.functions {
            if key != &function.instance {
                return Err(CanonicalMirError::FunctionKeyMismatch { key: *key, instance: function.instance });
            }
            if !linked.item_instances.contains_key(key) {
                return Err(CanonicalMirError::UnknownFunction { function: *key });
            }
            let mut defined = std::collections::BTreeSet::new();
            for (value, ty) in &function.parameters {
                if !linked.types.contains_key(ty) {
                    return Err(CanonicalMirError::UnknownType { function: *key, ty: *ty });
                }
                if !defined.insert(*value) {
                    return Err(CanonicalMirError::DuplicateDefinition { function: *key, value: *value });
                }
            }
            if !linked.types.contains_key(&function.return_type) {
                return Err(CanonicalMirError::UnknownType { function: *key, ty: function.return_type });
            }
            for call in &function.calls {
                if !linked.item_instances.contains_key(&call.callee) {
                    return Err(CanonicalMirError::UnknownCallee { function: *key, callee: call.callee });
                }
                if call.arguments.iter().any(|value| !defined.contains(value)) {
                    let value = *call.arguments.iter().find(|value| !defined.contains(value)).expect("missing argument");
                    return Err(CanonicalMirError::UseBeforeDefinition { function: *key, value });
                }
                if let Some(result) = call.result {
                    if !defined.insert(result) {
                        return Err(CanonicalMirError::DuplicateDefinition { function: *key, value: result });
                    }
                }
            }
        }
        Ok(())
    }
}

/// Top-level canonical success bundle after link + MIR validation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonicalProgram {
    /// Linked semantic closure.
    pub linked: LinkedSemanticProgram,
    /// Validated MIR (semantic only).
    pub mir: CanonicalSemanticMir,
}

impl CanonicalProgram {
    /// 验证链接闭包与 Semantic MIR 后，才允许进入 processing half。
    pub fn validate(&self) -> Result<(), CanonicalMirError> {
        self.mir.validate(&self.linked)
    }
}

/// One-way compile stream orchestration points (no God parallel authorities).
///
/// Implementations live in `nyar-language` / `nyar-emitter`; this module only
/// defines the stage contracts.
pub mod pipeline {
    use super::{CanonicalProgram, CompileStage, LinkedSemanticProgram, StageResult};
    use crate::semantic_ids::layout_choice::RepresentationPlan;

    /// Analysis / link stage: HIR elaboration consumed → linked program.
    pub trait LinkStage {
        /// Produce a closed linked program or structured diagnostics.
        fn link(&self) -> StageResult<LinkedSemanticProgram>;
    }

    /// M2 validation stage.
    pub trait ValidateMirStage {
        /// Validate linked program into canonical MIR success type.
        fn validate(&self, linked: &LinkedSemanticProgram) -> StageResult<CanonicalProgram>;
    }

    /// Representation planning stage (sparse side tables only).
    pub trait RepresentationPlanStage {
        /// Plan layouts without rewriting CFG or inventing semantics.
        fn plan(&self, program: &CanonicalProgram) -> StageResult<RepresentationPlan>;
    }

    /// Documented stage order for maintainers / agents.
    pub const STAGE_ORDER: &[CompileStage] = &[
        CompileStage::Ast,
        CompileStage::Hir,
        CompileStage::SemanticMir,
        CompileStage::LinkTime,
        CompileStage::ValidateMir,
        CompileStage::RepresentationPlan,
        CompileStage::BackendPrivatePlan,
        CompileStage::Emit,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic_ids::ItemInstanceId;

    #[test]
    fn canonical_program_is_success_only() {
        let mut linked = LinkedSemanticProgram::default();
        linked.module_name = "demo".into();
        let item = ItemInstanceId::from_index(0).unwrap();
        let ty = TypeId::from_index(0).unwrap();
        linked.types.insert(ty, TypeRecord { declaration: ty });
        linked.item_instances.insert(item, ItemInstanceRecord {
            declaration: ItemId::from_index(0).unwrap(),
            substitution: SubstitutionId::from_index(0).unwrap(),
        });
        let instance = ItemInstanceId::from_index(0).unwrap();
        let function = CanonicalFunction {
            instance,
            parameters: Vec::new(),
            return_type: ty,
            calls: Vec::new(),
        };
        let mut functions = BTreeMap::new();
        functions.insert(instance, function);
        let program = CanonicalProgram { linked, mir: CanonicalSemanticMir { module_name: "demo".into(), functions } };
        assert_eq!(program.mir.functions.len(), 1);
        program.mir.validate(&program.linked).expect("canonical MIR contract");
        program.validate().expect("canonical program contract");
    }

    #[test]
    fn canonical_mir_rejects_unknown_callee_before_planning() {
        let mut linked = LinkedSemanticProgram::default();
        let instance = ItemInstanceId::from_index(0).unwrap();
        let unknown = ItemInstanceId::from_index(1).unwrap();
        let ty = TypeId::from_index(0).unwrap();
        linked.item_instances.insert(instance, ItemInstanceRecord {
            declaration: ItemId::from_index(0).unwrap(),
            substitution: SubstitutionId::from_index(0).unwrap(),
        });
        linked.types.insert(ty, TypeRecord { declaration: ty });
        let function = CanonicalFunction {
            instance,
            parameters: Vec::new(),
            return_type: ty,
            calls: vec![CanonicalCall { callee: unknown, arguments: Vec::new(), result: None }],
        };
        let mut functions = BTreeMap::new();
        functions.insert(instance, function);
        let mir = CanonicalSemanticMir { module_name: "demo".into(), functions };
        assert!(matches!(mir.validate(&linked), Err(CanonicalMirError::UnknownCallee { .. })));
    }

    #[test]
    fn structured_diagnostic_set_rejects_empty() {
        assert!(StructuredDiagnosticSet::from_records(Vec::new()).is_none());
    }

    #[test]
    fn stage_order_is_one_way() {
        assert_eq!(pipeline::STAGE_ORDER[0], CompileStage::Ast);
        assert_eq!(*pipeline::STAGE_ORDER.last().unwrap(), CompileStage::Emit);
    }
}
