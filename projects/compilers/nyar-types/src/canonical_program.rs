//! 规范程序成功类型与编译管线阶段。
//!
//! 失败侧使用**结构化诊断**（共享合同的一族诊断类型），
//! 而不是名叫 `StructuredDiagnostics` 的单一结构体。

use crate::semantic_ids::{EvidenceId, FieldId, InstructionId, ItemId, ItemInstanceId, MirValueId, NominalInstanceId, SubstitutionId, TypeId, TypeInstanceId};
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
    /// 完成代入后的参数类型身份，顺序与调用 ABI 一致。
    pub parameter_types: Vec<TypeId>,
    /// 完成代入后的返回类型身份。
    pub return_type: TypeId,
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

/// Semantic MIR 中的稳定基本块身份。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CanonicalBlockId(pub u32);

/// Canonical Semantic MIR 的操作数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalOperand {
    /// 已定义的 SSA 值。
    Value(MirValueId),
    /// 语言常量。
    Constant(CanonicalConstant),
}

/// 不依赖目标的语言常量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalConstant {
    /// 有符号整数。
    Int(i64),
    /// 布尔值。
    Bool(bool),
    /// UTF-8 文本。
    Utf8(String),
    /// UTF-16 文本。
    Utf16(String),
    /// 单元值。
    Unit,
}

/// 已解析、已类型化的 Semantic MIR 操作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalOperation {
    /// 普通调用；callee 只能来自链接后的实例身份。
    Invoke { callee: ItemInstanceId, arguments: Vec<MirValueId> },
    /// SSA 值复制。
    Copy { source: MirValueId },
    /// 加载语言常量。
    LoadConstant { constant: CanonicalConstant },
    /// 构造名义聚合。
    StructNew { nominal: NominalInstanceId, fields: Vec<(FieldId, MirValueId)> },
    /// 读取已解析字段。
    FieldGet { object: MirValueId, field: FieldId },
    /// 写入已解析字段。
    FieldSet { object: MirValueId, field: FieldId, value: MirValueId },
    /// 从完整类型的数组读取。
    ArrayGet { array: MirValueId, index: MirValueId },
    /// 向完整类型的数组写入。
    ArraySet { array: MirValueId, index: MirValueId, value: MirValueId },
    /// 读取数组长度。
    ArrayLength { array: MirValueId },
}

/// 带稳定指令身份的 Semantic MIR 指令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalInstruction {
    /// 优化和表示规划使用的稳定指令身份。
    pub id: InstructionId,
    /// 本指令定义的 SSA 值。
    pub results: Vec<MirValueId>,
    /// 已完成身份解析的操作。
    pub operation: CanonicalOperation,
}

/// Semantic MIR 基本块终结符。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalTerminator {
    /// 返回函数结果。
    Return { value: Option<MirValueId> },
    /// 带显式并行块参数的跳转。
    Jump { target: CanonicalBlockId, arguments: Vec<MirValueId> },
    /// 条件分支。
    Branch { condition: MirValueId, then_target: CanonicalBlockId, else_target: CanonicalBlockId },
    /// 不可达终点。
    Unreachable,
}

/// 完整的 Semantic MIR 基本块。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalBlock {
    /// 稳定块身份。
    pub id: CanonicalBlockId,
    /// 块参数及其类型。
    pub parameters: Vec<(MirValueId, TypeId)>,
    /// 按语义执行顺序排列的指令。
    pub instructions: Vec<CanonicalInstruction>,
    /// 控制流终点。
    pub terminator: CanonicalTerminator,
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
    /// 函数内所有 SSA 值的稳定类型身份。
    pub value_types: BTreeMap<MirValueId, TypeId>,
    /// 函数入口块。
    pub entry: CanonicalBlockId,
    /// 完整 CFG；后端不得重建控制流或重新解析操作。
    pub blocks: BTreeMap<CanonicalBlockId, CanonicalBlock>,
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
    /// 调用实参数量与实例化签名不一致。
    CallArityMismatch { function: ItemInstanceId, callee: ItemInstanceId },
    /// 调用实参的 SSA 类型与实例化签名不一致。
    CallArgumentTypeMismatch { function: ItemInstanceId, callee: ItemInstanceId, value: MirValueId },
    /// 调用结果的 SSA 类型与实例化返回类型不一致。
    CallResultTypeMismatch { function: ItemInstanceId, callee: ItemInstanceId, value: MirValueId },
    /// 函数参数的 SSA 类型表记录不一致。
    ValueTypeMismatch { function: ItemInstanceId, value: MirValueId, expected: TypeId },
    /// 函数入口或返回合同与其实例化声明不一致。
    FunctionSignatureMismatch { function: ItemInstanceId },
    /// 基本块引用了不存在的目标。
    UnknownBlock { function: ItemInstanceId, block: CanonicalBlockId },
    /// 跳转参数数量与目标块参数不一致。
    BlockParameterArityMismatch { function: ItemInstanceId, block: CanonicalBlockId },
    /// 块参数或指令结果重复定义 SSA 值。
    DuplicateBlockDefinition { function: ItemInstanceId, value: MirValueId },
    /// 终结符使用了未定义的 SSA 值。
    TerminatorUseBeforeDefinition { function: ItemInstanceId, value: MirValueId },
}

impl CanonicalSemanticMir {
    /// 在进入 RepresentationPlan 前验证 stable-ID、链接和 SSA 合同。
    pub fn validate(&self, linked: &LinkedSemanticProgram) -> Result<(), CanonicalMirError> {
        for (key, function) in &self.functions {
            if key != &function.instance {
                return Err(CanonicalMirError::FunctionKeyMismatch { key: *key, instance: function.instance });
            }
            let Some(owner) = linked.item_instances.get(key) else {
                return Err(CanonicalMirError::UnknownFunction { function: *key });
            };
            if function.parameters.iter().map(|(_, ty)| *ty).collect::<Vec<_>>() != owner.parameter_types
                || function.return_type != owner.return_type
            {
                return Err(CanonicalMirError::FunctionSignatureMismatch { function: *key });
            }
            for ty in function.value_types.values() {
                if !linked.types.contains_key(ty) {
                    return Err(CanonicalMirError::UnknownType { function: *key, ty: *ty });
                }
            }
            let mut defined = std::collections::BTreeSet::new();
            for (value, ty) in &function.parameters {
                if !linked.types.contains_key(ty) {
                    return Err(CanonicalMirError::UnknownType { function: *key, ty: *ty });
                }
                if !defined.insert(*value) {
                    return Err(CanonicalMirError::DuplicateDefinition { function: *key, value: *value });
                }
                if function.value_types.get(value) != Some(ty) {
                    return Err(CanonicalMirError::ValueTypeMismatch { function: *key, value: *value, expected: *ty });
                }
            }
            if !linked.types.contains_key(&function.return_type) {
                return Err(CanonicalMirError::UnknownType { function: *key, ty: function.return_type });
            }
            if !function.blocks.contains_key(&function.entry) {
                return Err(CanonicalMirError::UnknownBlock { function: *key, block: function.entry });
            }
            for (block_id, block) in &function.blocks {
                let mut block_defined = defined.clone();
                for (value, ty) in &block.parameters {
                    if !linked.types.contains_key(ty) {
                        return Err(CanonicalMirError::UnknownType { function: *key, ty: *ty });
                    }
                    if !block_defined.insert(*value) {
                        return Err(CanonicalMirError::DuplicateBlockDefinition { function: *key, value: *value });
                    }
                    if function.value_types.get(value) != Some(ty) {
                        return Err(CanonicalMirError::ValueTypeMismatch { function: *key, value: *value, expected: *ty });
                    }
                }
                for instruction in &block.instructions {
                    let mut uses = Vec::new();
                    if let CanonicalOperation::Invoke { callee, arguments } = &instruction.operation {
                        let Some(callee_record) = linked.item_instances.get(callee) else {
                            return Err(CanonicalMirError::UnknownCallee { function: *key, callee: *callee });
                        };
                        for ty in callee_record.parameter_types.iter().chain(std::iter::once(&callee_record.return_type)) {
                            if !linked.types.contains_key(ty) {
                                return Err(CanonicalMirError::UnknownType { function: *key, ty: *ty });
                            }
                        }
                        if arguments.len() != callee_record.parameter_types.len() {
                            return Err(CanonicalMirError::CallArityMismatch { function: *key, callee: *callee });
                        }
                        uses.extend(arguments.iter().copied());
                        for (value, expected) in arguments.iter().zip(&callee_record.parameter_types) {
                            if function.value_types.get(value) != Some(expected) {
                                return Err(CanonicalMirError::CallArgumentTypeMismatch { function: *key, callee: *callee, value: *value });
                            }
                        }
                        if let Some(result) = instruction.results.first() {
                            if function.value_types.get(result) != Some(&callee_record.return_type) {
                                return Err(CanonicalMirError::CallResultTypeMismatch { function: *key, callee: *callee, value: *result });
                            }
                        }
                    }
                    if uses.iter().any(|value| !block_defined.contains(value)) {
                        let value = *uses.iter().find(|value| !block_defined.contains(value)).unwrap();
                        return Err(CanonicalMirError::UseBeforeDefinition { function: *key, value });
                    }
                    for result in &instruction.results {
                        if !block_defined.insert(*result) {
                            return Err(CanonicalMirError::DuplicateDefinition { function: *key, value: *result });
                        }
                        if !function.value_types.contains_key(result) {
                            return Err(CanonicalMirError::ValueTypeMismatch { function: *key, value: *result, expected: function.return_type });
                        }
                    }
                }
                let (targets, terminator_values): (Vec<_>, Vec<_>) = match &block.terminator {
                    CanonicalTerminator::Return { value } => (Vec::new(), value.iter().copied().collect()),
                    CanonicalTerminator::Jump { target, arguments } => (vec![(*target, arguments.len())], arguments.clone()),
                    CanonicalTerminator::Branch { condition, then_target, else_target } => (vec![(*then_target, 0), (*else_target, 0)], vec![*condition]),
                    CanonicalTerminator::Unreachable => (Vec::new(), Vec::new()),
                };
                for value in terminator_values {
                    if !block_defined.contains(&value) {
                        return Err(CanonicalMirError::TerminatorUseBeforeDefinition { function: *key, value });
                    }
                }
                for (target, arity) in targets {
                    let Some(target_block) = function.blocks.get(&target) else {
                        return Err(CanonicalMirError::UnknownBlock { function: *key, block: target });
                    };
                    if target_block.parameters.len() != arity {
                        return Err(CanonicalMirError::BlockParameterArityMismatch { function: *key, block: target });
                    }
                }
                let _ = block_id;
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
            parameter_types: Vec::new(),
            return_type: ty,
        });
        let instance = ItemInstanceId::from_index(0).unwrap();
        let function = CanonicalFunction {
            instance,
            parameters: Vec::new(),
            return_type: ty,
            value_types: BTreeMap::new(),
            entry: CanonicalBlockId(0),
            blocks: BTreeMap::from([(CanonicalBlockId(0), CanonicalBlock {
                id: CanonicalBlockId(0), parameters: Vec::new(), instructions: Vec::new(),
                terminator: CanonicalTerminator::Return { value: None },
            })]),
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
            parameter_types: Vec::new(),
            return_type: ty,
        });
        linked.types.insert(ty, TypeRecord { declaration: ty });
        let function = CanonicalFunction {
            instance,
            parameters: Vec::new(),
            return_type: ty,
            value_types: BTreeMap::new(),
            entry: CanonicalBlockId(0),
            blocks: BTreeMap::from([(CanonicalBlockId(0), CanonicalBlock {
                id: CanonicalBlockId(0), parameters: Vec::new(),
                instructions: vec![CanonicalInstruction {
                    id: InstructionId::from_index(0).unwrap(), results: Vec::new(),
                    operation: CanonicalOperation::Invoke { callee: unknown, arguments: Vec::new() },
                }],
                terminator: CanonicalTerminator::Return { value: None },
            })]),
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

    fn typed_call_program() -> CanonicalProgram {
        let caller = ItemInstanceId::from_index(0).unwrap();
        let callee = ItemInstanceId::from_index(1).unwrap();
        let ty = TypeId::from_index(0).unwrap();
        let argument = MirValueId::from_index(0).unwrap();
        let result = MirValueId::from_index(1).unwrap();
        let mut linked = LinkedSemanticProgram::default();
        linked.types.insert(ty, TypeRecord { declaration: ty });
        for instance in [caller, callee] {
            linked.item_instances.insert(instance, ItemInstanceRecord {
                declaration: ItemId::from_index(instance.index()).unwrap(),
                substitution: SubstitutionId::from_index(0).unwrap(),
                parameter_types: vec![ty],
                return_type: ty,
            });
        }
        let function = CanonicalFunction {
            instance: caller,
            parameters: vec![(argument, ty)],
            return_type: ty,
            value_types: BTreeMap::from([(argument, ty), (result, ty)]),
            entry: CanonicalBlockId(0),
            blocks: BTreeMap::from([(CanonicalBlockId(0), CanonicalBlock {
                id: CanonicalBlockId(0), parameters: Vec::new(),
                instructions: vec![CanonicalInstruction {
                    id: InstructionId::from_index(0).unwrap(), results: vec![result],
                    operation: CanonicalOperation::Invoke { callee, arguments: vec![argument] },
                }],
                terminator: CanonicalTerminator::Return { value: Some(result) },
            })]),
        };
        CanonicalProgram {
            linked,
            mir: CanonicalSemanticMir { module_name: "typed".into(), functions: BTreeMap::from([(caller, function)]) },
        }
    }

    #[test]
    fn canonical_call_requires_instantiated_signature() {
        let mut program = typed_call_program();
        program.validate().expect("完整实例化调用合同");
        if let CanonicalOperation::Invoke { arguments, .. } = &mut program.mir.functions.values_mut().next().unwrap().blocks.get_mut(&CanonicalBlockId(0)).unwrap().instructions[0].operation {
            arguments.clear();
        }
        assert!(matches!(program.validate(), Err(CanonicalMirError::CallArityMismatch { .. })));
    }

    #[test]
    fn canonical_call_rejects_argument_type_mismatch() {
        let mut program = typed_call_program();
        let other = TypeId::from_index(1).unwrap();
        program.linked.types.insert(other, TypeRecord { declaration: other });
        program.linked.item_instances.get_mut(&ItemInstanceId::from_index(1).unwrap()).unwrap().parameter_types[0] = other;
        assert!(matches!(program.validate(), Err(CanonicalMirError::CallArgumentTypeMismatch { .. })));
    }

    #[test]
    fn canonical_call_rejects_missing_result_type() {
        let mut program = typed_call_program();
        program.mir.functions.values_mut().next().unwrap().value_types.remove(&MirValueId::from_index(1).unwrap());
        assert!(matches!(program.validate(), Err(CanonicalMirError::CallResultTypeMismatch { .. })));
    }

    #[test]
    fn canonical_function_requires_declared_entry_signature() {
        let mut program = typed_call_program();
        program.mir.functions.values_mut().next().unwrap().parameters.clear();
        assert!(matches!(program.validate(), Err(CanonicalMirError::FunctionSignatureMismatch { .. })));
    }

    #[test]
    fn canonical_call_rejects_unlinked_signature_type() {
        let mut program = typed_call_program();
        program.linked.item_instances.get_mut(&ItemInstanceId::from_index(1).unwrap()).unwrap().return_type = TypeId::from_index(9).unwrap();
        assert!(matches!(program.validate(), Err(CanonicalMirError::UnknownType { .. })));
    }

    #[test]
    fn stage_order_is_one_way() {
        assert_eq!(pipeline::STAGE_ORDER[0], CompileStage::Ast);
        assert_eq!(*pipeline::STAGE_ORDER.last().unwrap(), CompileStage::Emit);
    }
}
