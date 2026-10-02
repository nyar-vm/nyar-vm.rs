//! 规范程序成功类型与编译管线阶段。
//!
//! 失败侧使用**结构化诊断**（共享合同的一族诊断类型），
//! 而不是名叫 `StructuredDiagnostics` 的单一结构体。

use crate::{AggregateLayoutPlan, FlagsLayout, QualifiedName, SumTypeLayout, semantic_ids::{
    layout_choice::RepresentationPlan,
    EvidenceId, FieldId, ImportCapability, ImportIndex, InstructionId, ItemId, ItemInstanceId, MirValueId, NominalInstanceId,
    SubstitutionId, TypeId, TypeInstanceId, ValueIdentity, VariantId,
}};
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
    /// Compiler 已解析的 callable identity 到限定 ABI 名称映射；后端不得从 MIR 文本反查。
    pub callable_names: BTreeMap<ItemInstanceId, QualifiedName>,
    /// Closed nominal ADT instances.
    pub nominal_instances: BTreeMap<NominalInstanceId, NominalInstanceRecord>,
    /// 已解析字段身份及其 owner/类型合同。
    pub fields: BTreeMap<FieldId, FieldRecord>,
    /// 声明 variant 在具体名义实例中的 payload 合同；不同实例不得覆盖同一声明身份。
    pub variants: BTreeMap<(NominalInstanceId, VariantId), VariantRecord>,
    /// Selected evidence bindings.
    pub evidence: BTreeMap<EvidenceId, EvidenceRecord>,
    /// 已绑定的外部导入槽；执行层只消费 `ImportIndex`。
    pub imports: BTreeMap<ImportIndex, ImportRecord>,
    /// 已解析的公开导出合同，键为 Compiler 分配的 callable identity。
    pub exports: BTreeMap<ItemInstanceId, ExportRecord>,
    /// 已解析的程序入口合同。
    pub entries: BTreeMap<ItemInstanceId, EntryRecord>,
    /// Semantic type table.
    pub types: BTreeMap<TypeId, TypeRecord>,
    /// Compiler 解析出的聚合布局；装配与 backend 不得从旧 MIR 重建。
    pub aggregate_layouts: AggregateLayoutPlan,
    /// Compiler 解析出的 sum 布局；装配与 backend 不得从名称猜测。
    pub sum_types: Vec<SumTypeLayout>,
    /// Compiler 解析出的 flags 布局。
    pub flags_types: Vec<FlagsLayout>,
}

/// 一个已链接 callable 的公开导出合同。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRecord {
    /// 公开 ABI 名称，仅用于互操作边界。
    pub exported_name: String,
}

/// 一个已链接 callable 的程序入口合同。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EntryRecord;

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

/// 名义类型声明的值语义；由语言前端决定，不是目标物理布局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NominalValueSemantics {
    /// 复制聚合值，不共享对象身份。
    Value,
    /// 复制引用，保留对象身份。
    Reference,
}

/// 完成实例化的名义类型合同。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NominalInstanceRecord {
    /// 名义类型声明身份。
    pub declaration: TypeId,
    /// 完成泛型代入的完整实例类型；不同实参不得共用字段或 variant 合同。
    pub ty: TypeId,
    /// 完成代入后的类型实例身份。
    pub substitution: SubstitutionId,
    /// 声明决定的值/引用语义，不能由字段数量或目标布局推断。
    pub semantics: NominalValueSemantics,
    /// 该实例声明的全部字段，顺序为语言声明顺序。
    pub fields: Vec<FieldId>,
}

/// 已解析字段的 owner 与值类型合同。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRecord {
    /// 字段所属名义实例。
    pub owner: NominalInstanceId,
    /// 字段值类型。
    pub ty: TypeId,
}

/// 已解析 sum variant 的 owner 与 payload 合同。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantRecord {
    /// 单 payload 类型；无 payload 时为空。
    pub payload_type: Option<TypeId>,
}

/// Placeholder evidence row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRecord {
    /// trait 或 adaptor 声明身份。
    pub trait_id: ItemId,
    /// 实现方的具体类型实例身份。
    pub implementing_type: TypeInstanceId,
}

/// 已完成签名绑定的外部导入记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRecord {
    /// Compiler 已选择并验证的外部链接合同。
    pub link: crate::ExternalImportLink,
    /// 互操作链接能力，仅用于链接和诊断，不作为执行分派键。
    pub capability: ImportCapability,
    /// 对应的外部 callable 实例。
    pub callee: ItemInstanceId,
    /// 已代入的参数类型。
    pub parameter_types: Vec<TypeId>,
    /// 已代入的返回类型。
    pub return_type: TypeId,
}

/// Canonical 类型的结构事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalTypeKind {
    /// 语言原生标量或单元类型。
    Primitive(CanonicalPrimitiveType),
    /// 名义类型及其已代入参数。
    Nominal { declaration: TypeId, arguments: Vec<TypeId> },
    /// 元组类型。
    Tuple(Vec<TypeId>),
    /// 数组类型。
    Array { element: TypeId, length: Option<u64> },
    /// 语言的显式可空类型；名义 Option 使用 Nominal 记录。
    Nullable(TypeId),
    /// 匿名联合类型。
    Union(Vec<TypeId>),
    /// 匿名交集类型。
    Intersection(Vec<TypeId>),
    /// 语言函数值类型。
    Function { parameters: Vec<TypeId>, return_type: TypeId },
}

/// Canonical 原生类型种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalPrimitiveType {
    /// 无返回值。
    Void,
    /// 布尔。
    Bool,
    /// 带宽度和符号的整数。
    Integer { bits: u16, signed: bool },
    /// 带宽度的浮点。
    Float { bits: u16 },
    /// Unicode 字符。
    Character,
    /// 文本。
    Utf8,
    /// UTF-16 文本。
    Utf16,
    /// 单元。
    Unit,
}

/// Canonical 类型表中的结构化记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRecord {
    /// 类型表中的声明身份。
    pub declaration: TypeId,
    /// 类型的完整语义形状。
    pub kind: CanonicalTypeKind,
}

impl LinkedSemanticProgram {
    /// 验证整个类型闭包，不限于当前函数引用到的行。
    pub fn validate_types(&self) -> Result<(), CanonicalMirError> {
        for (owner, record) in &self.types {
            let references = match &record.kind {
                CanonicalTypeKind::Primitive(primitive) => {
                    let valid = match primitive {
                        CanonicalPrimitiveType::Integer { bits, .. } => matches!(bits, 8 | 16 | 32 | 64 | 128),
                        CanonicalPrimitiveType::Float { bits } => matches!(bits, 32 | 64),
                        _ => true,
                    };
                    if !valid {
                        return Err(CanonicalMirError::InvalidPrimitiveType { ty: *owner });
                    }
                    Vec::new()
                }
                CanonicalTypeKind::Nominal { declaration, arguments } => {
                    std::iter::once(*declaration).chain(arguments.iter().copied()).collect()
                }
                CanonicalTypeKind::Tuple(members)
                | CanonicalTypeKind::Union(members)
                | CanonicalTypeKind::Intersection(members) => members.clone(),
                CanonicalTypeKind::Array { element, .. } | CanonicalTypeKind::Nullable(element) => vec![*element],
                CanonicalTypeKind::Function { parameters, return_type } => {
                    parameters.iter().copied().chain(std::iter::once(*return_type)).collect()
                }
            };
            for referenced in references {
                if !self.types.contains_key(&referenced) {
                    return Err(CanonicalMirError::UnknownTypeReference { owner: *owner, referenced });
                }
            }
        }
        let mut nominal_semantics = BTreeMap::new();
        let mut nominal_types = std::collections::BTreeSet::new();
        let mut declared_fields = std::collections::BTreeSet::new();
        for (nominal, record) in &self.nominal_instances {
            if !matches!(self.types.get(&record.declaration).map(|ty| &ty.kind), Some(CanonicalTypeKind::Nominal { declaration, .. }) if *declaration == record.declaration) {
                return Err(CanonicalMirError::InvalidNominalDeclaration { nominal: *nominal, declaration: record.declaration });
            }
            if !matches!(self.types.get(&record.ty).map(|ty| &ty.kind), Some(CanonicalTypeKind::Nominal { declaration, .. }) if *declaration == record.declaration) {
                return Err(CanonicalMirError::InvalidNominalInstanceType { nominal: *nominal, ty: record.ty });
            }
            if !nominal_types.insert(record.ty) {
                return Err(CanonicalMirError::DuplicateNominalInstanceType { ty: record.ty });
            }
            if let Some(previous) = nominal_semantics.insert(record.declaration, record.semantics) {
                if previous != record.semantics {
                    return Err(CanonicalMirError::ConflictingNominalSemantics { declaration: record.declaration });
                }
            }
            for field in &record.fields {
                if !declared_fields.insert(*field) || !self.fields.get(field).is_some_and(|row| row.owner == *nominal && self.types.contains_key(&row.ty)) {
                    return Err(CanonicalMirError::InvalidNominalField { nominal: *nominal, field: *field });
                }
            }
        }
        for (field, record) in &self.fields {
            if !declared_fields.contains(field) {
                return Err(CanonicalMirError::InvalidNominalField { nominal: record.owner, field: *field });
            }
        }
        for record in self.types.values() {
            if let CanonicalTypeKind::Nominal { declaration, .. } = &record.kind {
                if !nominal_semantics.contains_key(declaration) {
                    return Err(CanonicalMirError::MissingNominalSemantics { declaration: *declaration });
                }
            }
        }
        for (ty, record) in &self.types {
            let references = match &record.kind {
                CanonicalTypeKind::Nominal { declaration, arguments } => {
                    if ty != declaration || !arguments.is_empty() {
                        self.validate_nominal_value_type(*ty, &nominal_types)?;
                    }
                    arguments.clone()
                }
                CanonicalTypeKind::Tuple(members)
                | CanonicalTypeKind::Union(members)
                | CanonicalTypeKind::Intersection(members) => members.clone(),
                CanonicalTypeKind::Array { element, .. } | CanonicalTypeKind::Nullable(element) => vec![*element],
                CanonicalTypeKind::Function { parameters, return_type } => {
                    parameters.iter().copied().chain(std::iter::once(*return_type)).collect()
                }
                CanonicalTypeKind::Primitive(_) => Vec::new(),
            };
            for referenced in references {
                self.validate_nominal_value_type(referenced, &nominal_types)?;
            }
        }
        for record in self.item_instances.values() {
            for ty in record.parameter_types.iter().chain(std::iter::once(&record.return_type)) {
                self.validate_nominal_value_type(*ty, &nominal_types)?;
            }
        }
        for field in self.fields.values() {
            self.validate_nominal_value_type(field.ty, &nominal_types)?;
        }
        for ((nominal, variant_id), variant) in &self.variants {
            if !self.nominal_instances.contains_key(nominal) {
                return Err(CanonicalMirError::InvalidVariantOwner { nominal: *nominal, variant: *variant_id });
            }
            if let Some(ty) = variant.payload_type {
                self.validate_nominal_value_type(ty, &nominal_types)?;
            }
        }
        for import in self.imports.values() {
            let Some(callee) = self.item_instances.get(&import.callee) else {
                return Err(CanonicalMirError::UnknownFunction { function: import.callee });
            };
            if callee.parameter_types != import.parameter_types || callee.return_type != import.return_type {
                return Err(CanonicalMirError::ImportSignatureMismatch { function: import.callee });
            }
            for ty in import.parameter_types.iter().chain(std::iter::once(&import.return_type)) {
                if !self.types.contains_key(ty) {
                    return Err(CanonicalMirError::UnknownType { function: import.callee, ty: *ty });
                }
            }
        }
        Ok(())
    }

    fn validate_nominal_value_type(&self, ty: TypeId, nominal_types: &std::collections::BTreeSet<TypeId>) -> Result<(), CanonicalMirError> {
        if matches!(self.types.get(&ty).map(|record| &record.kind), Some(CanonicalTypeKind::Nominal { .. }))
            && !nominal_types.contains(&ty)
        {
            return Err(CanonicalMirError::MissingNominalInstance { ty });
        }
        Ok(())
    }
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
    /// 普通调用；callee 是已链接项或已类型化函数值。
    Invoke { callee: CanonicalCallee, arguments: Vec<MirValueId> },
    /// SSA 值复制。
    Copy { source: MirValueId },
    /// 按已解析聚合类型复制值。
    AggregateCopy { source: MirValueId, destination: MirValueId },
    /// 加载语言常量。
    LoadConstant { constant: CanonicalConstant },
    /// 构造已解析的 sum variant。
    SumNew { nominal: NominalInstanceId, variant: VariantId, payload: Option<MirValueId> },
    /// 提取已解析 variant 的 payload。
    SumPayloadGet { nominal: NominalInstanceId, variant: VariantId, object: MirValueId },
    /// 检查 sum 值是否为已解析 variant。
    SumVariantIs { nominal: NominalInstanceId, variant: VariantId, object: MirValueId },
    /// 构造名义聚合。
    StructNew { nominal: NominalInstanceId, fields: Vec<(FieldId, MirValueId)> },
    /// 读取已解析字段。
    FieldGet { object: MirValueId, field: FieldId },
    /// 写入已解析字段。
    FieldSet { object: MirValueId, field: FieldId, value: MirValueId },
    /// 从完整类型的数组读取。
    ArrayGet { array: MirValueId, index: MirValueId },
    /// 按完整类型和语言初始化规则构造数组。
    ArrayNew { array_type: TypeId, length: MirValueId, initialization: CanonicalArrayInitialization },
    /// 从完整元素序列构造数组。
    ArrayFromElements { array_type: TypeId, elements: Vec<MirValueId> },
    /// 向完整类型的数组写入。
    ArraySet { array: MirValueId, index: MirValueId, value: MirValueId },
    /// 读取数组长度。
    ArrayLength { array: MirValueId },
    /// 构造元组值。
    TupleNew { element_types: Vec<TypeId>, fields: Vec<MirValueId> },
}

/// 已完成类型检查的调用目标。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalCallee {
    /// 链接后的静态/实例 callable。
    Item(ItemInstanceId),
    /// 当前 SSA 中的函数值。
    Value(MirValueId),
}

/// 数组的语言初始化合同。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalArrayInitialization {
    /// 使用语言定义的元素默认值。
    Default,
    /// 使用已求值的 SSA 值填充所有元素。
    Fill(MirValueId),
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
    /// 执行已解析 effect，并转移到恢复块。
    PerformEffect { effect: CanonicalEffectKind, payload: Option<MirValueId>, resume_target: CanonicalBlockId },
    /// 按已解析状态值分发到恢复块。
    StateDispatch { state: MirValueId, cases: Vec<(u32, CanonicalBlockId)>, default_target: CanonicalBlockId },
    /// 将 effect 控制权交还 runtime。
    YieldToRuntime { effect: CanonicalEffectKind, payload: Option<MirValueId>, resume_state: u32 },
    /// 不可达终点。
    Unreachable,
}

/// Semantic MIR 中不依赖目标的 effect 类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalEffectKind {
    /// `raise expr`。
    Raise,
    /// `yield expr`。
    Yield,
    /// `yield from expr`。
    DelegateYield,
    /// `expr.await`。
    Await,
    /// `expr.awake`。
    AsyncSpawn,
    /// `expr.block`。
    AsyncBlock,
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
    /// 名义类型没有来自声明的值/引用语义。
    MissingNominalSemantics { declaration: TypeId },
    /// 值类型没有对应完整 TypeId 的名义实例合同。
    MissingNominalInstance { ty: TypeId },
    /// 名义实例没有对应的名义声明类型。
    InvalidNominalDeclaration { nominal: NominalInstanceId, declaration: TypeId },
    /// 实例类型未知、非名义类型或不属于该声明。
    InvalidNominalInstanceType { nominal: NominalInstanceId, ty: TypeId },
    /// 同一完整实例类型被重复登记为不同名义实例。
    DuplicateNominalInstanceType { ty: TypeId },
    /// 同一声明的实例携带了互相矛盾的值/引用语义。
    ConflictingNominalSemantics { declaration: TypeId },
    /// 字段清单重复、缺失或与 owner/类型合同不一致。
    InvalidNominalField { nominal: NominalInstanceId, field: FieldId },
    /// 公开导出或程序入口未指向本闭包中的函数体。
    MissingSurfaceBody { function: ItemInstanceId },
    /// 不同 callable 使用了同一公开导出名。
    DuplicateExportName { name: String },
    /// effect 终结符缺少语义 payload。
    MissingEffectPayload { function: ItemInstanceId, block: CanonicalBlockId },
    /// 状态分发值不是整数类型。
    InvalidDispatchState { function: ItemInstanceId, value: MirValueId },
    /// 状态分发中同一状态存在多个目标。
    DuplicateDispatchState { function: ItemInstanceId, block: CanonicalBlockId, state: u32 },
    /// 聚合实例身份未知。
    UnknownNominal { function: ItemInstanceId, nominal: NominalInstanceId },
    /// 外部导入记录与 callable 签名不一致。
    ImportSignatureMismatch { function: ItemInstanceId },
    /// 类型形状引用了不存在的类型。
    UnknownTypeReference { owner: TypeId, referenced: TypeId },
    /// 原生标量宽度不属于语言类型合同。
    InvalidPrimitiveType { ty: TypeId },
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
    /// 函数值 callee 的 SSA 类型不是函数类型。
    ValueCalleeNotFunction { function: ItemInstanceId, value: MirValueId },
    /// 函数值 callee 的实参数量不一致。
    ValueCalleeArityMismatch { function: ItemInstanceId, value: MirValueId },
    /// 函数值 callee 的实参类型不一致。
    ValueCalleeArgumentTypeMismatch { function: ItemInstanceId, value: MirValueId, argument: MirValueId },
    /// 函数值 callee 的结果类型不一致。
    ValueCalleeResultTypeMismatch { function: ItemInstanceId, value: MirValueId, result: MirValueId },
    /// 函数参数的 SSA 类型表记录不一致。
    ValueTypeMismatch { function: ItemInstanceId, value: MirValueId, expected: TypeId },
    /// 函数入口或返回合同与其实例化声明不一致。
    FunctionSignatureMismatch { function: ItemInstanceId },
    /// 返回值类型与函数合同不一致。
    ReturnTypeMismatch { function: ItemInstanceId, value: MirValueId },
    /// 基本块引用了不存在的目标。
    UnknownBlock { function: ItemInstanceId, block: CanonicalBlockId },
    /// 跳转参数数量与目标块参数不一致。
    BlockParameterArityMismatch { function: ItemInstanceId, block: CanonicalBlockId },
    /// 块参数或指令结果重复定义 SSA 值。
    DuplicateBlockDefinition { function: ItemInstanceId, value: MirValueId },
    /// 终结符使用了未定义的 SSA 值。
    TerminatorUseBeforeDefinition { function: ItemInstanceId, value: MirValueId },
    /// SSA 定义块不支配使用块。
    NonDominatingUse { function: ItemInstanceId, value: MirValueId, block: CanonicalBlockId },
    /// 跳转实参与目标块参数类型不一致。
    BlockParameterTypeMismatch { function: ItemInstanceId, block: CanonicalBlockId, value: MirValueId },
    /// 聚合字段身份未知。
    UnknownField { function: ItemInstanceId, field: FieldId },
    /// 字段不属于当前聚合 owner。
    FieldOwnerMismatch { function: ItemInstanceId, field: FieldId, nominal: NominalInstanceId },
    /// 字段值类型与声明不一致。
    FieldTypeMismatch { function: ItemInstanceId, field: FieldId, value: MirValueId },
    /// 聚合构造缺少声明字段。
    MissingStructField { function: ItemInstanceId, nominal: NominalInstanceId, field: FieldId },
    /// 聚合构造重复声明字段。
    DuplicateStructField { function: ItemInstanceId, nominal: NominalInstanceId, field: FieldId },
    /// sum variant 身份未知。
    UnknownVariant { function: ItemInstanceId, variant: VariantId },
    /// sum variant 的具体名义 owner 不存在。
    InvalidVariantOwner { nominal: NominalInstanceId, variant: VariantId },
    /// sum variant payload 与声明合同不一致。
    VariantPayloadMismatch { function: ItemInstanceId, variant: VariantId },
    /// 结构操作的结果类型与其声明合同不一致。
    OperationResultTypeMismatch { function: ItemInstanceId, instruction: InstructionId },
    /// 结构操作的输入类型与其声明合同不一致。
    OperationOperandTypeMismatch { function: ItemInstanceId, instruction: InstructionId, value: MirValueId },
    /// 结构操作的结果个数与其语义合同不一致。
    OperationResultArityMismatch { function: ItemInstanceId, instruction: InstructionId },
    /// 结构操作的输入个数与其类型合同不一致。
    OperationOperandArityMismatch { function: ItemInstanceId, instruction: InstructionId },
}

impl CanonicalSemanticMir {
    /// 在进入 RepresentationPlan 前验证 stable-ID、链接和 SSA 合同。
    pub fn validate(&self, linked: &LinkedSemanticProgram) -> Result<(), CanonicalMirError> {
        linked.validate_types()?;
        let nominal_types = linked.nominal_instances.values().map(|record| record.ty).collect();
        let mut export_names = std::collections::BTreeSet::new();
        for (function, export) in &linked.exports {
            if !linked.item_instances.contains_key(function) || !self.functions.contains_key(function) {
                return Err(CanonicalMirError::MissingSurfaceBody { function: *function });
            }
            if !export_names.insert(&export.exported_name) {
                return Err(CanonicalMirError::DuplicateExportName { name: export.exported_name.clone() });
            }
        }
        for function in linked.entries.keys() {
            if !linked.item_instances.contains_key(function) || !self.functions.contains_key(function) {
                return Err(CanonicalMirError::MissingSurfaceBody { function: *function });
            }
        }
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
                linked.validate_nominal_value_type(*ty, &nominal_types)?;
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
            let all_blocks = function.blocks.keys().copied().collect::<std::collections::BTreeSet<_>>();
            let mut predecessors = all_blocks.iter().map(|block| (*block, std::collections::BTreeSet::new())).collect::<BTreeMap<_, _>>();
            for block in function.blocks.values() {
                let targets = match &block.terminator {
                    CanonicalTerminator::Jump { target, .. } => vec![*target],
                    CanonicalTerminator::Branch { then_target, else_target, .. } => vec![*then_target, *else_target],
                    CanonicalTerminator::PerformEffect { resume_target, .. } => vec![*resume_target],
                    CanonicalTerminator::StateDispatch { cases, default_target, .. } => {
                        cases.iter().map(|(_, target)| *target).chain(std::iter::once(*default_target)).collect()
                    }
                    _ => Vec::new(),
                };
                for target in targets {
                    if let Some(preds) = predecessors.get_mut(&target) { preds.insert(block.id); }
                }
            }
            let mut dominators = all_blocks.iter().map(|block| {
                let initial = if *block == function.entry { std::collections::BTreeSet::from([*block]) } else { all_blocks.clone() };
                (*block, initial)
            }).collect::<BTreeMap<_, _>>();
            loop {
                let mut changed = false;
                for block in all_blocks.iter().copied().filter(|block| *block != function.entry) {
                    let mut next = all_blocks.clone();
                    for predecessor in predecessors.get(&block).into_iter().flat_map(|values| values.iter()) {
                        next = next.intersection(dominators.get(predecessor).unwrap()).copied().collect();
                    }
                    next.insert(block);
                    if next != dominators[&block] { dominators.insert(block, next); changed = true; }
                }
                if !changed { break; }
            }
            let mut definitions = function.parameters.iter().map(|(value, _)| (*value, None)).collect::<BTreeMap<_, _>>();
            for block in function.blocks.values() {
                for (value, _) in &block.parameters {
                    if block.id == function.entry {
                        if !function.parameters.iter().any(|(parameter, _)| parameter == value) {
                            return Err(CanonicalMirError::DuplicateDefinition { function: *key, value: *value });
                        }
                        continue;
                    }
                    if definitions.insert(*value, Some(block.id)).is_some() {
                        return Err(CanonicalMirError::DuplicateDefinition { function: *key, value: *value });
                    }
                }
                for value in block.instructions.iter().flat_map(|instruction| instruction.results.iter()) {
                    if definitions.insert(*value, Some(block.id)).is_some() {
                        return Err(CanonicalMirError::DuplicateDefinition { function: *key, value: *value });
                    }
                }
            }
            for (block_id, block) in &function.blocks {
                let mut block_defined = defined.clone();
                for (index, (value, ty)) in block.parameters.iter().enumerate() {
                    if !linked.types.contains_key(ty) {
                        return Err(CanonicalMirError::UnknownType { function: *key, ty: *ty });
                    }
                    if block.id == function.entry {
                        if function.parameters.get(index) != Some(&(*value, *ty)) {
                            return Err(CanonicalMirError::ValueTypeMismatch { function: *key, value: *value, expected: *ty });
                        }
                        continue;
                    }
                    if !block_defined.insert(*value) {
                        return Err(CanonicalMirError::DuplicateBlockDefinition { function: *key, value: *value });
                    }
                    if function.value_types.get(value) != Some(ty) {
                        return Err(CanonicalMirError::ValueTypeMismatch { function: *key, value: *value, expected: *ty });
                    }
                }
                for instruction in &block.instructions {
                    let uses = match &instruction.operation {
                        CanonicalOperation::Invoke { callee, arguments } => {
                            match callee {
                                CanonicalCallee::Item(callee) => {
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
                                let returns_unit = matches!(linked.types.get(&callee_record.return_type).map(|row| &row.kind), Some(CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Unit | CanonicalPrimitiveType::Void)));
                                if instruction.results.len() > 1 || (!returns_unit && instruction.results.is_empty()) {
                                    return Err(CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id });
                                }
                                }
                                CanonicalCallee::Value(callee) => {
                                let Some(callee_type) = function.value_types.get(callee) else {
                                    return Err(CanonicalMirError::UseBeforeDefinition { function: *key, value: *callee });
                                };
                                let Some(CanonicalTypeKind::Function { parameters, return_type }) = linked.types.get(callee_type).map(|record| &record.kind) else {
                                    return Err(CanonicalMirError::ValueCalleeNotFunction { function: *key, value: *callee });
                                };
                                if arguments.len() != parameters.len() {
                                    return Err(CanonicalMirError::ValueCalleeArityMismatch { function: *key, value: *callee });
                                }
                                for (argument, expected) in arguments.iter().zip(parameters) {
                                    if function.value_types.get(argument) != Some(expected) {
                                        return Err(CanonicalMirError::ValueCalleeArgumentTypeMismatch { function: *key, value: *callee, argument: *argument });
                                    }
                                }
                                if let Some(result) = instruction.results.first() {
                                    if function.value_types.get(result) != Some(return_type) {
                                        return Err(CanonicalMirError::ValueCalleeResultTypeMismatch { function: *key, value: *callee, result: *result });
                                    }
                                }
                                let returns_unit = matches!(linked.types.get(return_type).map(|row| &row.kind), Some(CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Unit | CanonicalPrimitiveType::Void)));
                                if instruction.results.len() > 1 || (!returns_unit && instruction.results.is_empty()) {
                                    return Err(CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id });
                                }
                                }
                            }
                            let mut uses = arguments.clone();
                            if let CanonicalCallee::Value(callee) = callee {
                                uses.push(*callee);
                            }
                            uses
                        }
                    CanonicalOperation::Copy { source } => {
                        if instruction.results.len() != 1 || instruction.results.first().and_then(|value| function.value_types.get(value)) != function.value_types.get(source) {
                            return Err(if instruction.results.len() == 1 { CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id } } else { CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id } });
                        }
                        vec![*source]
                    }
                    CanonicalOperation::AggregateCopy { source, destination } => {
                        let source_type = function.value_types.get(source);
                        let destination_type = function.value_types.get(destination);
                        if source_type.is_none() || source_type != destination_type {
                            return Err(CanonicalMirError::ValueTypeMismatch { function: *key, value: *destination, expected: source_type.copied().unwrap_or(function.return_type) });
                        }
                        if instruction.results.as_slice() != [*destination] {
                            return Err(CanonicalMirError::DuplicateDefinition { function: *key, value: *destination });
                        }
                        vec![*source]
                    }
                    CanonicalOperation::LoadConstant { .. } => {
                        if instruction.results.len() != 1 {
                            return Err(CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id });
                        }
                        Vec::new()
                    }
                    CanonicalOperation::SumNew { nominal, variant, payload } => {
                        let Some(record) = linked.variants.get(&(*nominal, *variant)) else {
                            return Err(CanonicalMirError::UnknownVariant { function: *key, variant: *variant });
                        };
                        let Some(owner) = linked.nominal_instances.get(nominal) else {
                            return Err(CanonicalMirError::UnknownNominal { function: *key, nominal: *nominal });
                        };
                        let result_type = instruction.results.first().and_then(|value| function.value_types.get(value));
                        let result_is_owner = result_type == Some(&owner.ty);
                        if instruction.results.len() != 1 || !result_is_owner {
                            return Err(if instruction.results.len() == 1 {
                                CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                            } else {
                                CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                            });
                        }
                        match (record.payload_type, payload) {
                            (Some(expected), Some(value)) if function.value_types.get(value) == Some(&expected) => vec![*value],
                            (None, None) => Vec::new(),
                            _ => return Err(CanonicalMirError::VariantPayloadMismatch { function: *key, variant: *variant }),
                        }
                    }
                    CanonicalOperation::SumPayloadGet { nominal, variant, object } => {
                        let Some(record) = linked.variants.get(&(*nominal, *variant)) else {
                            return Err(CanonicalMirError::UnknownVariant { function: *key, variant: *variant });
                        };
                        if record.payload_type.is_none() {
                            return Err(CanonicalMirError::VariantPayloadMismatch { function: *key, variant: *variant });
                        }
                        if instruction.results.len() != 1 || instruction.results.first().and_then(|value| function.value_types.get(value)) != record.payload_type.as_ref() {
                            return Err(if instruction.results.len() == 1 {
                                CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                            } else {
                                CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                            });
                        }
                        let Some(object_type) = function.value_types.get(object) else {
                            return Err(CanonicalMirError::UseBeforeDefinition { function: *key, value: *object });
                        };
                        let Some(owner) = linked.nominal_instances.get(nominal) else {
                            return Err(CanonicalMirError::UnknownNominal { function: *key, nominal: *nominal });
                        };
                        if *object_type != owner.ty {
                            return Err(CanonicalMirError::OperationOperandTypeMismatch { function: *key, instruction: instruction.id, value: *object });
                        }
                        vec![*object]
                    }
                    CanonicalOperation::SumVariantIs { nominal, variant, object } => {
                        let Some(_record) = linked.variants.get(&(*nominal, *variant)) else {
                            return Err(CanonicalMirError::UnknownVariant { function: *key, variant: *variant });
                        };
                        let bool_result = instruction.results.len() == 1 && instruction.results.first().and_then(|value| function.value_types.get(value)).is_some_and(|ty| matches!(linked.types.get(ty).map(|row| &row.kind), Some(CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Bool))));
                        if !bool_result {
                            return Err(if instruction.results.len() == 1 {
                                CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                            } else {
                                CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                            });
                        }
                        let Some(object_type) = function.value_types.get(object) else {
                            return Err(CanonicalMirError::UseBeforeDefinition { function: *key, value: *object });
                        };
                        let Some(owner) = linked.nominal_instances.get(nominal) else {
                            return Err(CanonicalMirError::UnknownNominal { function: *key, nominal: *nominal });
                        };
                        if *object_type != owner.ty {
                            return Err(CanonicalMirError::OperationOperandTypeMismatch { function: *key, instruction: instruction.id, value: *object });
                        }
                        vec![*object]
                    }
                    CanonicalOperation::StructNew { nominal, fields } => {
                        let Some(nominal_record) = linked.nominal_instances.get(nominal) else {
                            return Err(CanonicalMirError::UnknownNominal { function: *key, nominal: *nominal });
                        };
                        let mut seen = std::collections::BTreeSet::new();
                        for (field, value) in fields {
                            if !seen.insert(*field) {
                                return Err(CanonicalMirError::DuplicateStructField { function: *key, nominal: *nominal, field: *field });
                            }
                            let Some(record) = linked.fields.get(field) else {
                                return Err(CanonicalMirError::UnknownField { function: *key, field: *field });
                            };
                            if record.owner != *nominal {
                                return Err(CanonicalMirError::FieldOwnerMismatch { function: *key, field: *field, nominal: *nominal });
                            }
                            if function.value_types.get(value) != Some(&record.ty) {
                                return Err(CanonicalMirError::FieldTypeMismatch { function: *key, field: *field, value: *value });
                            }
                        }
                        for field in &nominal_record.fields {
                            if !seen.contains(field) {
                                return Err(CanonicalMirError::MissingStructField { function: *key, nominal: *nominal, field: *field });
                            }
                        }
                        if instruction.results.len() != 1 || instruction.results.first().and_then(|value| function.value_types.get(value)) != Some(&nominal_record.ty) {
                            return Err(if instruction.results.len() == 1 {
                                CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }
                            } else {
                                CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }
                            });
                        }
                        fields.iter().map(|(_, value)| *value).collect()
                    }
                    CanonicalOperation::FieldGet { object, field } => {
                        let Some(record) = linked.fields.get(field) else {
                            return Err(CanonicalMirError::UnknownField { function: *key, field: *field });
                        };
                        let Some(object_ty) = function.value_types.get(object) else {
                            return Err(CanonicalMirError::UseBeforeDefinition { function: *key, value: *object });
                        };
                        let Some(owner) = linked.nominal_instances.get(&record.owner) else {
                            return Err(CanonicalMirError::UnknownNominal { function: *key, nominal: record.owner });
                        };
                        if *object_ty != owner.ty {
                            return Err(CanonicalMirError::FieldOwnerMismatch { function: *key, field: *field, nominal: record.owner });
                        }
                        if instruction.results.first().and_then(|value| function.value_types.get(value)) != Some(&record.ty) {
                            return Err(CanonicalMirError::FieldTypeMismatch { function: *key, field: *field, value: instruction.results.first().copied().unwrap_or(*object) });
                        }
                        vec![*object]
                    }
                    CanonicalOperation::FieldSet { object, field, value } => {
                        if !instruction.results.is_empty() {
                            return Err(CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id });
                        }
                        let Some(record) = linked.fields.get(field) else {
                            return Err(CanonicalMirError::UnknownField { function: *key, field: *field });
                        };
                        let Some(object_ty) = function.value_types.get(object) else {
                            return Err(CanonicalMirError::UseBeforeDefinition { function: *key, value: *object });
                        };
                        let Some(owner) = linked.nominal_instances.get(&record.owner) else {
                            return Err(CanonicalMirError::UnknownNominal { function: *key, nominal: record.owner });
                        };
                        if *object_ty != owner.ty {
                            return Err(CanonicalMirError::FieldOwnerMismatch { function: *key, field: *field, nominal: record.owner });
                        }
                        if function.value_types.get(value) != Some(&record.ty) {
                            return Err(CanonicalMirError::FieldTypeMismatch { function: *key, field: *field, value: *value });
                        }
                        vec![*object, *value]
                    }
                    CanonicalOperation::ArrayGet { array, index } => {
                        let Some(CanonicalTypeKind::Array { element, .. }) = function.value_types.get(array).and_then(|ty| linked.types.get(ty)).map(|row| &row.kind) else {
                            return Err(CanonicalMirError::OperationOperandTypeMismatch { function: *key, instruction: instruction.id, value: *array });
                        };
                        let index_is_integer = function.value_types.get(index).and_then(|ty| linked.types.get(ty)).is_some_and(|row| matches!(row.kind, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { .. })));
                        if !index_is_integer {
                            return Err(CanonicalMirError::OperationOperandTypeMismatch { function: *key, instruction: instruction.id, value: *index });
                        }
                        if instruction.results.len() != 1 || instruction.results.first().and_then(|value| function.value_types.get(value)) != Some(element) {
                            return Err(if instruction.results.len() == 1 { CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id } } else { CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id } });
                        }
                        vec![*array, *index]
                    }
                    CanonicalOperation::ArrayNew { array_type, length, initialization } => {
                        let Some(CanonicalTypeKind::Array { element, .. }) = linked.types.get(array_type).map(|row| &row.kind) else {
                            return Err(CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id });
                        };
                        let length_is_integer = function.value_types.get(length).and_then(|ty| linked.types.get(ty)).is_some_and(|row| matches!(row.kind, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { .. })));
                        let fill_is_valid = match initialization {
                            CanonicalArrayInitialization::Default => true,
                            CanonicalArrayInitialization::Fill(value) => function.value_types.get(value) == Some(element),
                        };
                        if !length_is_integer || !fill_is_valid {
                            return Err(CanonicalMirError::OperationOperandTypeMismatch { function: *key, instruction: instruction.id, value: *length });
                        }
                        if instruction.results.len() != 1 || instruction.results.first().and_then(|value| function.value_types.get(value)) != Some(array_type) {
                            return Err(if instruction.results.len() == 1 { CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id } } else { CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id } });
                        }
                        match initialization { CanonicalArrayInitialization::Default => vec![*length], CanonicalArrayInitialization::Fill(value) => vec![*length, *value] }
                    }
                    CanonicalOperation::ArrayFromElements { array_type, elements } => {
                        let Some(CanonicalTypeKind::Array { element, .. }) = linked.types.get(array_type).map(|row| &row.kind) else { return Err(CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id }); };
                        if instruction.results.len() != 1 || instruction.results.first().and_then(|value| function.value_types.get(value)) != Some(array_type) { return Err(if instruction.results.len() == 1 { CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id } } else { CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id } }); }
                        if elements.iter().any(|value| function.value_types.get(value) != Some(element)) { return Err(CanonicalMirError::OperationOperandTypeMismatch { function: *key, instruction: instruction.id, value: *elements.iter().find(|value| function.value_types.get(value) != Some(element)).unwrap() }); }
                        elements.clone()
                    }
                    CanonicalOperation::ArraySet { array, index, value } => {
                        let Some(CanonicalTypeKind::Array { element, .. }) = function.value_types.get(array).and_then(|ty| linked.types.get(ty)).map(|row| &row.kind) else { return Err(CanonicalMirError::OperationOperandTypeMismatch { function: *key, instruction: instruction.id, value: *array }); };
                        let index_is_integer = function.value_types.get(index).and_then(|ty| linked.types.get(ty)).is_some_and(|row| matches!(row.kind, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { .. })));
                        if !index_is_integer || function.value_types.get(value) != Some(element) { return Err(CanonicalMirError::OperationOperandTypeMismatch { function: *key, instruction: instruction.id, value: if !index_is_integer { *index } else { *value } }); }
                        if !instruction.results.is_empty() { return Err(CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }); }
                        vec![*array, *index, *value]
                    }
                    CanonicalOperation::ArrayLength { array } => {
                        if !matches!(function.value_types.get(array).and_then(|ty| linked.types.get(ty)).map(|row| &row.kind), Some(CanonicalTypeKind::Array { .. })) { return Err(CanonicalMirError::OperationOperandTypeMismatch { function: *key, instruction: instruction.id, value: *array }); }
                        let result_is_integer = instruction.results.len() == 1 && instruction.results.first().and_then(|value| function.value_types.get(value)).and_then(|ty| linked.types.get(ty)).is_some_and(|row| matches!(row.kind, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { .. })));
                        if !result_is_integer { return Err(if instruction.results.len() == 1 { CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id } } else { CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id } }); }
                        vec![*array]
                    }
                    CanonicalOperation::TupleNew { element_types, fields } => {
                        let Some(result) = instruction.results.first().and_then(|value| function.value_types.get(value)) else { return Err(CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id }); };
                        if instruction.results.len() != 1 || !matches!(linked.types.get(result).map(|row| &row.kind), Some(CanonicalTypeKind::Tuple(members)) if members == element_types) { return Err(if instruction.results.len() == 1 { CanonicalMirError::OperationResultTypeMismatch { function: *key, instruction: instruction.id } } else { CanonicalMirError::OperationResultArityMismatch { function: *key, instruction: instruction.id } }); }
                        if fields.len() != element_types.len() {
                            return Err(CanonicalMirError::OperationOperandArityMismatch { function: *key, instruction: instruction.id });
                        }
                        if let Some((value, _)) = fields.iter().zip(element_types).find(|(value, expected)| function.value_types.get(value) != Some(expected)) {
                            return Err(CanonicalMirError::OperationOperandTypeMismatch { function: *key, instruction: instruction.id, value: *value });
                        }
                        fields.clone()
                    }
                    };
                    if uses.iter().any(|value| !block_defined.contains(value)) {
                        let value = *uses.iter().find(|value| !block_defined.contains(value)).unwrap();
                        return Err(CanonicalMirError::UseBeforeDefinition { function: *key, value });
                    }
                    for value in &uses {
                        if let Some(Some(definition_block)) = definitions.get(value) {
                            if definition_block != block_id && !dominators[block_id].contains(definition_block) {
                                return Err(CanonicalMirError::NonDominatingUse { function: *key, value: *value, block: *block_id });
                            }
                        }
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
                    CanonicalTerminator::Return { value } => {
                        if let Some(value) = value {
                            if function.value_types.get(value) != Some(&function.return_type) {
                                return Err(CanonicalMirError::ReturnTypeMismatch { function: *key, value: *value });
                            }
                        }
                        (Vec::new(), value.iter().copied().collect())
                    }
                    CanonicalTerminator::Jump { target, arguments } => (vec![(*target, arguments.len())], arguments.clone()),
                    CanonicalTerminator::Branch { condition, then_target, else_target } => (vec![(*then_target, 0), (*else_target, 0)], vec![*condition]),
                    CanonicalTerminator::PerformEffect { effect, resume_target, payload } => {
                        let payload = payload.ok_or(CanonicalMirError::MissingEffectPayload { function: *key, block: *block_id })?;
                        let resume_arity = usize::from(*effect != CanonicalEffectKind::AsyncSpawn);
                        (vec![(*resume_target, resume_arity)], vec![payload])
                    }
                    CanonicalTerminator::StateDispatch { state, cases, default_target } => {
                        let state_type = function.value_types.get(state).and_then(|ty| linked.types.get(ty));
                        if !matches!(state_type.map(|record| &record.kind), Some(CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { .. }))) {
                            return Err(CanonicalMirError::InvalidDispatchState { function: *key, value: *state });
                        }
                        let mut states = std::collections::BTreeSet::new();
                        for (state, _) in cases {
                            if !states.insert(*state) {
                                return Err(CanonicalMirError::DuplicateDispatchState { function: *key, block: *block_id, state: *state });
                            }
                        }
                        (cases.iter().map(|(_, target)| (*target, 0)).chain(std::iter::once((*default_target, 0))).collect(), vec![*state])
                    }
                    CanonicalTerminator::YieldToRuntime { payload, .. } => {
                        let payload = payload.ok_or(CanonicalMirError::MissingEffectPayload { function: *key, block: *block_id })?;
                        (Vec::new(), vec![payload])
                    }
                    CanonicalTerminator::Unreachable => (Vec::new(), Vec::new()),
                };
                for value in terminator_values {
                    if !block_defined.contains(&value) {
                        return Err(CanonicalMirError::TerminatorUseBeforeDefinition { function: *key, value });
                    }
                    if let Some(Some(definition_block)) = definitions.get(&value) {
                        if definition_block != block_id && !dominators[block_id].contains(definition_block) {
                            return Err(CanonicalMirError::NonDominatingUse { function: *key, value, block: *block_id });
                        }
                    }
                }
                for (target, arity) in targets {
                    let Some(target_block) = function.blocks.get(&target) else {
                        return Err(CanonicalMirError::UnknownBlock { function: *key, block: target });
                    };
                    if target_block.parameters.len() != arity {
                        return Err(CanonicalMirError::BlockParameterArityMismatch { function: *key, block: target });
                    }
                    if let CanonicalTerminator::Jump { arguments, .. } = &block.terminator {
                        for (value, (_, expected)) in arguments.iter().zip(&target_block.parameters) {
                            if function.value_types.get(value) != Some(expected) {
                                return Err(CanonicalMirError::BlockParameterTypeMismatch { function: *key, block: target, value: *value });
                            }
                        }
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

/// 编译处理阶段的唯一成功载荷。
///
/// 该类型只允许把已验证的 Canonical Semantic MIR 与对应的稀疏表示计划
/// 一起交给后续阶段。它不包含 HIR、字符串 callable、旧 executable 或
/// `FragmentSubmission` 字段；后端若需要这些信息，说明上游合同仍未闭合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledProgram {
    canonical: CanonicalProgram,
    representation: RepresentationPlan,
}

/// `CompiledProgram` 构造时发现的跨阶段合同错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompiledProgramError {
    /// 已链接的名义实例缺少表示选择。
    MissingAdtRepresentation { nominal: NominalInstanceId },
    /// 表示计划引用了程序之外的名义实例。
    ExtraAdtRepresentation { nominal: NominalInstanceId },
    /// Canonical Semantic MIR 尚未通过 M2 验证。
    Canonical(CanonicalMirError),
    /// 某个 Semantic MIR 值没有对应表示选择。
    MissingValueRepresentation { function: ItemInstanceId, value: MirValueId },
    /// 某条调用指令没有对应调用表示选择。
    MissingInvokeRepresentation { instruction: InstructionId },
    /// 表示计划包含不属于该程序的值键。
    ExtraValueRepresentation { function: ItemInstanceId, value: MirValueId },
    /// 表示计划包含不属于该程序的调用键。
    ExtraInvokeRepresentation { instruction: InstructionId },
}

impl CompiledProgram {
    /// 只从完整 CanonicalProgram 与同一程序生成的 RepresentationPlan 构造。
    pub fn new(canonical: CanonicalProgram, representation: RepresentationPlan) -> Result<Self, CompiledProgramError> {
        canonical.validate().map_err(CompiledProgramError::Canonical)?;
        for nominal in canonical.linked.nominal_instances.keys() {
            if !representation.adt_reps.contains_key(nominal) {
                return Err(CompiledProgramError::MissingAdtRepresentation { nominal: *nominal });
            }
        }
        if let Some(nominal) = representation.adt_reps.keys().find(|nominal| !canonical.linked.nominal_instances.contains_key(nominal)) {
            return Err(CompiledProgramError::ExtraAdtRepresentation { nominal: *nominal });
        }
        let mut expected_values = std::collections::BTreeSet::new();
        let mut expected_invokes = std::collections::BTreeSet::new();
        for function in canonical.mir.functions.values() {
            for value in function.value_types.keys() {
                let identity = ValueIdentity::new(function.instance, *value);
                expected_values.insert(identity);
                if !representation.value_reps.contains_key(&identity) {
                    return Err(CompiledProgramError::MissingValueRepresentation { function: function.instance, value: *value });
                }
            }
            for block in function.blocks.values() {
                for instruction in &block.instructions {
                    if matches!(&instruction.operation, CanonicalOperation::Invoke { .. })
                    {
                        expected_invokes.insert(instruction.id);
                        if !representation.invoke_lowerings.contains_key(&instruction.id) {
                            return Err(CompiledProgramError::MissingInvokeRepresentation { instruction: instruction.id });
                        }
                    }
                }
            }
        }
        if let Some(identity) = representation.value_reps.keys().find(|identity| !expected_values.contains(identity)) {
            return Err(CompiledProgramError::ExtraValueRepresentation { function: identity.function, value: identity.value });
        }
        if let Some(instruction) = representation.invoke_lowerings.keys().find(|instruction| !expected_invokes.contains(instruction)) {
            return Err(CompiledProgramError::ExtraInvokeRepresentation { instruction: *instruction });
        }
        Ok(Self { canonical, representation })
    }

    /// 返回 Compiler 产生的 Canonical Semantic MIR。
    pub fn canonical(&self) -> &CanonicalProgram {
        &self.canonical
    }

    /// 返回与 CanonicalProgram 同步生成的表示计划。
    pub fn representation(&self) -> &RepresentationPlan {
        &self.representation
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
        linked.types.insert(ty, TypeRecord { declaration: ty, kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Unit) });
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
        linked.types.insert(ty, TypeRecord { declaration: ty, kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Unit) });
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
                    operation: CanonicalOperation::Invoke { callee: CanonicalCallee::Item(unknown), arguments: Vec::new() },
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

    #[test]
    fn canonical_array_contract_checks_inputs_and_results_before_planning() {
        let instance = ItemInstanceId::from_index(0).unwrap();
        let integer = TypeId::from_index(0).unwrap();
        let boolean = TypeId::from_index(1).unwrap();
        let array_type = TypeId::from_index(2).unwrap();
        let array = MirValueId::from_index(0).unwrap();
        let index = MirValueId::from_index(1).unwrap();
        let result = MirValueId::from_index(2).unwrap();
        let instruction = InstructionId::from_index(0).unwrap();
        let mut linked = LinkedSemanticProgram::default();
        for (ty, kind) in [
            (integer, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { bits: 32, signed: true })),
            (boolean, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Bool)),
            (array_type, CanonicalTypeKind::Array { element: integer, length: None }),
        ] {
            linked.types.insert(ty, TypeRecord { declaration: ty, kind });
        }
        linked.item_instances.insert(instance, ItemInstanceRecord {
            declaration: ItemId::from_index(0).unwrap(), substitution: SubstitutionId::from_index(0).unwrap(),
            parameter_types: vec![array_type, integer], return_type: integer,
        });
        let function = CanonicalFunction {
            instance, parameters: vec![(array, array_type), (index, integer)], return_type: integer,
            value_types: BTreeMap::from([(array, array_type), (index, integer), (result, integer)]),
            entry: CanonicalBlockId(0),
            blocks: BTreeMap::from([(CanonicalBlockId(0), CanonicalBlock {
                id: CanonicalBlockId(0), parameters: Vec::new(),
                instructions: vec![CanonicalInstruction { id: instruction, results: vec![result], operation: CanonicalOperation::ArrayGet { array, index } }],
                terminator: CanonicalTerminator::Return { value: Some(result) },
            })]),
        };
        let program = CanonicalProgram { linked, mir: CanonicalSemanticMir { module_name: "array".into(), functions: BTreeMap::from([(instance, function)]) } };
        program.validate().expect("完整数组读取合同");
        let mut invalid = program.clone();
        invalid.mir.functions.get_mut(&instance).unwrap().value_types.insert(result, boolean);
        assert_eq!(invalid.validate(), Err(CanonicalMirError::OperationResultTypeMismatch { function: instance, instruction }));
        let mut invalid = program.clone();
        invalid.mir.functions.get_mut(&instance).unwrap().blocks.get_mut(&CanonicalBlockId(0)).unwrap().instructions[0].results.clear();
        assert_eq!(invalid.validate(), Err(CanonicalMirError::OperationResultArityMismatch { function: instance, instruction }));
        let mut invalid = program;
        invalid.mir.functions.get_mut(&instance).unwrap().blocks.get_mut(&CanonicalBlockId(0)).unwrap().instructions[0].operation = CanonicalOperation::ArrayGet { array: index, index };
        assert_eq!(invalid.validate(), Err(CanonicalMirError::OperationOperandTypeMismatch { function: instance, instruction, value: index }));
    }

    #[test]
    fn canonical_effect_terminator_preserves_payload_and_resume_contract() {
        let instance = ItemInstanceId::from_index(0).unwrap();
        let ty = TypeId::from_index(0).unwrap();
        let payload = MirValueId::from_index(0).unwrap();
        let resume_value = MirValueId::from_index(1).unwrap();
        let mut linked = LinkedSemanticProgram::default();
        linked.types.insert(ty, TypeRecord { declaration: ty, kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Unit) });
        linked.item_instances.insert(instance, ItemInstanceRecord {
            declaration: ItemId::from_index(0).unwrap(),
            substitution: SubstitutionId::from_index(0).unwrap(),
            parameter_types: vec![ty],
            return_type: ty,
        });
        let function = CanonicalFunction {
            instance,
            parameters: vec![(payload, ty)],
            return_type: ty,
            value_types: BTreeMap::from([(payload, ty), (resume_value, ty)]),
            entry: CanonicalBlockId(0),
            blocks: BTreeMap::from([
                (CanonicalBlockId(0), CanonicalBlock {
                    id: CanonicalBlockId(0), parameters: Vec::new(), instructions: Vec::new(),
                    terminator: CanonicalTerminator::PerformEffect {
                        effect: CanonicalEffectKind::Raise, payload: Some(payload), resume_target: CanonicalBlockId(1),
                    },
                }),
                (CanonicalBlockId(1), CanonicalBlock {
                    id: CanonicalBlockId(1), parameters: vec![(resume_value, ty)], instructions: Vec::new(),
                    terminator: CanonicalTerminator::Return { value: None },
                }),
            ]),
        };
        let program = CanonicalSemanticMir { module_name: "effect".into(), functions: BTreeMap::from([(instance, function)]) };
        program.validate(&linked).expect("effect payload and resume block are complete");
    }

    #[test]
    fn canonical_effect_terminator_rejects_missing_payload() {
        let instance = ItemInstanceId::from_index(0).unwrap();
        let ty = TypeId::from_index(0).unwrap();
        let mut linked = LinkedSemanticProgram::default();
        linked.types.insert(ty, TypeRecord { declaration: ty, kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Unit) });
        linked.item_instances.insert(instance, ItemInstanceRecord {
            declaration: ItemId::from_index(0).unwrap(),
            substitution: SubstitutionId::from_index(0).unwrap(),
            parameter_types: Vec::new(),
            return_type: ty,
        });
        let function = CanonicalFunction {
            instance,
            parameters: Vec::new(),
            return_type: ty,
            value_types: BTreeMap::new(),
            entry: CanonicalBlockId(0),
            blocks: BTreeMap::from([(CanonicalBlockId(0), CanonicalBlock {
                id: CanonicalBlockId(0), parameters: Vec::new(), instructions: Vec::new(),
                terminator: CanonicalTerminator::PerformEffect {
                    effect: CanonicalEffectKind::Raise, payload: None, resume_target: CanonicalBlockId(0),
                },
            })]),
        };
        let program = CanonicalSemanticMir { module_name: "effect".into(), functions: BTreeMap::from([(instance, function)]) };
        assert!(matches!(program.validate(&linked), Err(CanonicalMirError::MissingEffectPayload { .. })));
    }

    fn typed_call_program() -> CanonicalProgram {
        let caller = ItemInstanceId::from_index(0).unwrap();
        let callee = ItemInstanceId::from_index(1).unwrap();
        let ty = TypeId::from_index(0).unwrap();
        let argument = MirValueId::from_index(0).unwrap();
        let result = MirValueId::from_index(1).unwrap();
        let mut linked = LinkedSemanticProgram::default();
        linked.types.insert(ty, TypeRecord { declaration: ty, kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Unit) });
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
                    operation: CanonicalOperation::Invoke { callee: CanonicalCallee::Item(callee), arguments: vec![argument] },
                }],
                terminator: CanonicalTerminator::Return { value: Some(result) },
            })]),
        };
        CanonicalProgram {
            linked,
            mir: CanonicalSemanticMir { module_name: "typed".into(), functions: BTreeMap::from([(caller, function)]) },
        }
    }

    fn nominal_program() -> CanonicalProgram {
        let declaration = TypeId::from_index(0).unwrap();
        let field_type = TypeId::from_index(1).unwrap();
        let nominal = NominalInstanceId::from_index(0).unwrap();
        let field = FieldId::from_index(0).unwrap();
        let mut program = CanonicalProgram { linked: LinkedSemanticProgram::default(), mir: CanonicalSemanticMir::default() };
        program.linked.types.insert(declaration, TypeRecord {
            declaration, kind: CanonicalTypeKind::Nominal { declaration, arguments: Vec::new() },
        });
        program.linked.types.insert(field_type, TypeRecord {
            declaration: field_type, kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Bool),
        });
        program.linked.nominal_instances.insert(nominal, NominalInstanceRecord {
            declaration, ty: declaration, substitution: SubstitutionId::from_index(0).unwrap(),
            semantics: NominalValueSemantics::Reference, fields: vec![field],
        });
        program.linked.fields.insert(field, FieldRecord { owner: nominal, ty: field_type });
        program
    }

    #[test]
    fn canonical_nominal_contract_validates_the_entire_field_closure() {
        let program = nominal_program();
        program.validate().expect("没有构造指令的名义声明仍须验证");
        let nominal = NominalInstanceId::from_index(0).unwrap();
        let field = FieldId::from_index(0).unwrap();
        let mut invalid = program.clone();
        invalid.linked.nominal_instances.get_mut(&nominal).unwrap().fields.push(field);
        assert_eq!(invalid.validate(), Err(CanonicalMirError::InvalidNominalField { nominal, field }));
        let mut invalid = program.clone();
        invalid.linked.fields.get_mut(&field).unwrap().owner = NominalInstanceId::from_index(1).unwrap();
        assert!(matches!(invalid.validate(), Err(CanonicalMirError::InvalidNominalField { .. })));
        let mut invalid = program.clone();
        invalid.linked.fields.get_mut(&field).unwrap().ty = TypeId::from_index(9).unwrap();
        assert_eq!(invalid.validate(), Err(CanonicalMirError::InvalidNominalField { nominal, field }));
        let mut invalid = program.clone();
        invalid.linked.nominal_instances.get_mut(&nominal).unwrap().fields.clear();
        assert_eq!(invalid.validate(), Err(CanonicalMirError::InvalidNominalField { nominal, field }));
        let mut invalid = program.clone();
        invalid.linked.nominal_instances.clear();
        invalid.linked.fields.clear();
        assert_eq!(invalid.validate(), Err(CanonicalMirError::MissingNominalSemantics { declaration: TypeId::from_index(0).unwrap() }));
        let mut invalid = program;
        invalid.linked.nominal_instances.get_mut(&nominal).unwrap().declaration = TypeId::from_index(1).unwrap();
        assert!(matches!(invalid.validate(), Err(CanonicalMirError::InvalidNominalDeclaration { .. })));
    }

    #[test]
    fn canonical_nominal_instances_cannot_disagree_on_declaration_semantics() {
        let mut program = nominal_program();
        let mut other = program.linked.nominal_instances.values().next().unwrap().clone();
        other.fields.clear();
        other.substitution = SubstitutionId::from_index(1).unwrap();
        other.ty = TypeId::from_index(2).unwrap();
        program.linked.types.insert(other.ty, TypeRecord {
            declaration: other.declaration,
            kind: CanonicalTypeKind::Nominal { declaration: other.declaration, arguments: vec![TypeId::from_index(1).unwrap()] },
        });
        let instance = NominalInstanceId::from_index(1).unwrap();
        program.linked.nominal_instances.insert(instance, other.clone());
        program.validate().expect("同一声明不同实例保持共同值语义");
        program.linked.nominal_instances.get_mut(&instance).unwrap().semantics = NominalValueSemantics::Value;
        assert_eq!(program.validate(), Err(CanonicalMirError::ConflictingNominalSemantics { declaration: other.declaration }));
    }

    #[test]
    fn compiled_program_requires_exact_nominal_representation_coverage() {
        let program = nominal_program();
        let nominal = NominalInstanceId::from_index(0).unwrap();
        let mut plan = RepresentationPlan::default();
        assert_eq!(CompiledProgram::new(program.clone(), plan.clone()), Err(CompiledProgramError::MissingAdtRepresentation { nominal }));
        plan.adt_reps.insert(nominal, crate::layout_choice::AdtRepresentation::TypedAggregate);
        CompiledProgram::new(program.clone(), plan.clone()).expect("名义表示完整");
        let extra = NominalInstanceId::from_index(9).unwrap();
        plan.adt_reps.insert(extra, crate::layout_choice::AdtRepresentation::TypedAggregate);
        assert_eq!(CompiledProgram::new(program, plan), Err(CompiledProgramError::ExtraAdtRepresentation { nominal: extra }));
    }

    fn instantiated_nominal_operation_program(operation: CanonicalOperation, result_type: Option<TypeId>) -> CanonicalProgram {
        let mut program = nominal_program();
        let declaration = TypeId::from_index(0).unwrap();
        let boolean = TypeId::from_index(1).unwrap();
        let first_type = TypeId::from_index(2).unwrap();
        let second_type = TypeId::from_index(3).unwrap();
        let integer = TypeId::from_index(4).unwrap();
        program.linked.types.insert(integer, TypeRecord {
            declaration: integer, kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { bits: 32, signed: true }),
        });
        for (ty, argument) in [(first_type, boolean), (second_type, integer)] {
            program.linked.types.insert(ty, TypeRecord {
                declaration, kind: CanonicalTypeKind::Nominal { declaration, arguments: vec![argument] },
            });
        }
        let first = NominalInstanceId::from_index(0).unwrap();
        let second = NominalInstanceId::from_index(1).unwrap();
        program.linked.nominal_instances.get_mut(&first).unwrap().ty = first_type;
        let second_field = FieldId::from_index(1).unwrap();
        program.linked.nominal_instances.insert(second, NominalInstanceRecord {
            declaration, ty: second_type, substitution: SubstitutionId::from_index(1).unwrap(),
            semantics: NominalValueSemantics::Reference, fields: vec![second_field],
        });
        program.linked.fields.insert(second_field, FieldRecord { owner: second, ty: boolean });
        for (index, owner) in [(0, first), (1, second)] {
            program.linked.variants.insert((owner, VariantId::from_index(index).unwrap()), VariantRecord { payload_type: Some(boolean) });
        }
        let instance = ItemInstanceId::from_index(0).unwrap();
        let object = MirValueId::from_index(0).unwrap();
        let payload = MirValueId::from_index(1).unwrap();
        let result = MirValueId::from_index(2).unwrap();
        let return_type = result_type.unwrap_or(boolean);
        program.linked.item_instances.insert(instance, ItemInstanceRecord {
            declaration: ItemId::from_index(0).unwrap(), substitution: SubstitutionId::from_index(0).unwrap(),
            parameter_types: vec![first_type, boolean], return_type,
        });
        let mut value_types = BTreeMap::from([(object, first_type), (payload, boolean)]);
        if let Some(ty) = result_type {
            value_types.insert(result, ty);
        }
        let entry = CanonicalBlockId(0);
        program.mir.functions.insert(instance, CanonicalFunction {
            instance, parameters: vec![(object, first_type), (payload, boolean)], return_type,
            value_types, entry, blocks: BTreeMap::from([(entry, CanonicalBlock {
                id: entry, parameters: Vec::new(),
                instructions: vec![CanonicalInstruction {
                    id: InstructionId::from_index(0).unwrap(), results: result_type.map(|_| vec![result]).unwrap_or_default(), operation,
                }],
                terminator: CanonicalTerminator::Return { value: Some(if result_type.is_some() { result } else { payload }) },
            })]),
        });
        program
    }

    #[test]
    fn canonical_structure_operations_use_exact_instantiated_owner_types() {
        let first = NominalInstanceId::from_index(0).unwrap();
        let second = NominalInstanceId::from_index(1).unwrap();
        let first_field = FieldId::from_index(0).unwrap();
        let second_field = FieldId::from_index(1).unwrap();
        let first_variant = VariantId::from_index(0).unwrap();
        let second_variant = VariantId::from_index(1).unwrap();
        let object = MirValueId::from_index(0).unwrap();
        let payload = MirValueId::from_index(1).unwrap();
        let boolean = TypeId::from_index(1).unwrap();
        let first_type = TypeId::from_index(2).unwrap();
        let cases = [
            (CanonicalOperation::StructNew { nominal: first, fields: vec![(first_field, payload)] },
             CanonicalOperation::StructNew { nominal: second, fields: vec![(second_field, payload)] }, Some(first_type)),
            (CanonicalOperation::FieldGet { object, field: first_field },
             CanonicalOperation::FieldGet { object, field: second_field }, Some(boolean)),
            (CanonicalOperation::FieldSet { object, field: first_field, value: payload },
             CanonicalOperation::FieldSet { object, field: second_field, value: payload }, None),
            (CanonicalOperation::SumNew { nominal: first, variant: first_variant, payload: Some(payload) },
             CanonicalOperation::SumNew { nominal: second, variant: second_variant, payload: Some(payload) }, Some(first_type)),
            (CanonicalOperation::SumPayloadGet { nominal: first, variant: first_variant, object },
             CanonicalOperation::SumPayloadGet { nominal: second, variant: second_variant, object }, Some(boolean)),
            (CanonicalOperation::SumVariantIs { nominal: first, variant: first_variant, object },
             CanonicalOperation::SumVariantIs { nominal: second, variant: second_variant, object }, Some(boolean)),
        ];
        for (valid, wrong_owner, result_type) in cases {
            instantiated_nominal_operation_program(valid, result_type).validate().expect("同一声明的不同泛型实例可独立持有结构合同");
            let error = instantiated_nominal_operation_program(wrong_owner, result_type).validate().expect_err("字段形状相同不能掩盖泛型实例错位");
            assert!(matches!(error,
                CanonicalMirError::FieldOwnerMismatch { .. }
                | CanonicalMirError::OperationResultTypeMismatch { .. }
                | CanonicalMirError::OperationOperandTypeMismatch { .. }
            ), "{error:?}");
        }
    }

    #[test]
    fn canonical_sum_declaration_variant_has_independent_instance_payloads() {
        let first = NominalInstanceId::from_index(0).unwrap();
        let second = NominalInstanceId::from_index(1).unwrap();
        let variant = VariantId::from_index(0).unwrap();
        let object = MirValueId::from_index(0).unwrap();
        let boolean = TypeId::from_index(1).unwrap();
        let integer = TypeId::from_index(4).unwrap();
        let operation = CanonicalOperation::SumPayloadGet { nominal: first, variant, object };
        let mut program = instantiated_nominal_operation_program(operation, Some(boolean));
        program.linked.variants.remove(&(second, VariantId::from_index(1).unwrap()));
        program.linked.variants.insert((second, variant), VariantRecord { payload_type: Some(integer) });
        program.validate().expect("同一声明 variant 在两个实例中保留不同 payload，不覆盖第一实例");
        assert_eq!(program.linked.variants[&(first, variant)].payload_type, Some(boolean));
        assert_eq!(program.linked.variants[&(second, variant)].payload_type, Some(integer));
        let instance = ItemInstanceId::from_index(0).unwrap();
        program.mir.functions.get_mut(&instance).unwrap().blocks.get_mut(&CanonicalBlockId(0)).unwrap()
            .instructions[0].operation = CanonicalOperation::SumVariantIs { nominal: second, variant, object };
        assert!(matches!(program.validate(), Err(CanonicalMirError::OperationOperandTypeMismatch { .. })));
    }

    #[test]
    fn canonical_sum_registry_rejects_missing_owner_without_an_operation() {
        let mut program = nominal_program();
        let nominal = NominalInstanceId::from_index(9).unwrap();
        let variant = VariantId::from_index(0).unwrap();
        program.linked.variants.insert((nominal, variant), VariantRecord { payload_type: None });
        assert_eq!(program.validate(), Err(CanonicalMirError::InvalidVariantOwner { nominal, variant }));
    }

    #[test]
    fn canonical_nominal_instance_coverage_precedes_operations() {
        let field = FieldId::from_index(0).unwrap();
        let boolean = TypeId::from_index(1).unwrap();
        let object = MirValueId::from_index(0).unwrap();
        let program = instantiated_nominal_operation_program(CanonicalOperation::FieldGet { object, field }, Some(boolean));
        program.validate().expect("两个完整实例均有独立合同");
        let mut missing = program.clone();
        missing.linked.nominal_instances.remove(&NominalInstanceId::from_index(1).unwrap());
        missing.linked.fields.remove(&FieldId::from_index(1).unwrap());
        missing.linked.variants.remove(&(NominalInstanceId::from_index(1).unwrap(), VariantId::from_index(1).unwrap()));
        assert_eq!(missing.validate(), Err(CanonicalMirError::MissingNominalInstance { ty: TypeId::from_index(3).unwrap() }));

        let declaration = TypeId::from_index(0).unwrap();
        let instance = ItemInstanceId::from_index(0).unwrap();
        let mut signature = program.clone();
        signature.linked.item_instances.get_mut(&instance).unwrap().parameter_types[0] = declaration;
        assert_eq!(signature.validate(), Err(CanonicalMirError::MissingNominalInstance { ty: declaration }));
        let mut value = program.clone();
        value.mir.functions.get_mut(&instance).unwrap().value_types.insert(object, declaration);
        assert_eq!(value.validate(), Err(CanonicalMirError::MissingNominalInstance { ty: declaration }));
        let mut field_type = program.clone();
        field_type.linked.fields.get_mut(&field).unwrap().ty = declaration;
        assert_eq!(field_type.validate(), Err(CanonicalMirError::MissingNominalInstance { ty: declaration }));
        let mut payload = program.clone();
        payload.linked.variants.get_mut(&(NominalInstanceId::from_index(0).unwrap(), VariantId::from_index(0).unwrap())).unwrap().payload_type = Some(declaration);
        assert_eq!(payload.validate(), Err(CanonicalMirError::MissingNominalInstance { ty: declaration }));

        let wrapper = TypeId::from_index(9).unwrap();
        let shapes = [
            CanonicalTypeKind::Array { element: declaration, length: None },
            CanonicalTypeKind::Array { element: declaration, length: Some(2) },
            CanonicalTypeKind::Tuple(vec![declaration]),
            CanonicalTypeKind::Nullable(declaration),
            CanonicalTypeKind::Union(vec![declaration, boolean]),
            CanonicalTypeKind::Intersection(vec![declaration, boolean]),
            CanonicalTypeKind::Function { parameters: vec![declaration], return_type: boolean },
            CanonicalTypeKind::Function { parameters: vec![boolean], return_type: declaration },
        ];
        for kind in shapes {
            let mut nested = program.clone();
            nested.linked.types.insert(wrapper, TypeRecord { declaration: wrapper, kind });
            assert_eq!(nested.validate(), Err(CanonicalMirError::MissingNominalInstance { ty: declaration }));
        }
    }

    #[test]
    fn canonical_recursive_nominal_contract_does_not_expand_types() {
        let mut program = nominal_program();
        let declaration = TypeId::from_index(0).unwrap();
        program.linked.fields.get_mut(&FieldId::from_index(0).unwrap()).unwrap().ty = declaration;
        program.validate().expect("递归字段只验证现有闭包，不递归实例化或展开布局");
    }

    #[test]
    fn canonical_nominal_instance_type_is_required_and_unique() {
        let nominal = NominalInstanceId::from_index(0).unwrap();
        let mut program = nominal_program();
        let unknown = TypeId::from_index(9).unwrap();
        program.linked.nominal_instances.get_mut(&nominal).unwrap().ty = unknown;
        assert_eq!(program.validate(), Err(CanonicalMirError::InvalidNominalInstanceType { nominal, ty: unknown }));
        let mut program = nominal_program();
        program.linked.nominal_instances.insert(NominalInstanceId::from_index(1).unwrap(), program.linked.nominal_instances[&nominal].clone());
        assert_eq!(program.validate(), Err(CanonicalMirError::DuplicateNominalInstanceType { ty: TypeId::from_index(0).unwrap() }));
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
    fn canonical_surface_requires_a_body_for_each_export_and_entry() {
        let caller = ItemInstanceId::from_index(0).unwrap();
        let external = ItemInstanceId::from_index(1).unwrap();
        let mut program = typed_call_program();
        program.linked.exports.insert(caller, ExportRecord { exported_name: "public_answer".into() });
        program.linked.entries.insert(caller, EntryRecord);
        program.validate().expect("完整公开合同");
        program.linked.exports.insert(external, ExportRecord { exported_name: "external_answer".into() });
        assert_eq!(program.validate(), Err(CanonicalMirError::MissingSurfaceBody { function: external }));
        program.linked.exports.remove(&external);
        program.linked.entries.insert(external, EntryRecord);
        assert_eq!(program.validate(), Err(CanonicalMirError::MissingSurfaceBody { function: external }));
        program.linked.entries.remove(&external);
        let unknown = ItemInstanceId::from_index(9).unwrap();
        program.linked.entries.insert(unknown, EntryRecord);
        assert_eq!(program.validate(), Err(CanonicalMirError::MissingSurfaceBody { function: unknown }));
    }

    #[test]
    fn canonical_surface_rejects_duplicate_public_names_but_allows_multiple_exports() {
        let caller = ItemInstanceId::from_index(0).unwrap();
        let other = ItemInstanceId::from_index(1).unwrap();
        let mut program = typed_call_program();
        let mut function = program.mir.functions[&caller].clone();
        function.instance = other;
        program.mir.functions.insert(other, function);
        program.linked.exports.insert(caller, ExportRecord { exported_name: "first_answer".into() });
        program.linked.exports.insert(other, ExportRecord { exported_name: "second_answer".into() });
        program.validate().expect("多导出完整合同");
        program.linked.exports.get_mut(&other).unwrap().exported_name = "first_answer".into();
        assert_eq!(program.validate(), Err(CanonicalMirError::DuplicateExportName { name: "first_answer".into() }));
    }

    #[test]
    fn compiled_program_requires_complete_representation_plan() {
        let program = typed_call_program();
        let error = CompiledProgram::new(program, RepresentationPlan::default()).expect_err("缺少值表示必须在处理边界失败");
        assert!(matches!(error, CompiledProgramError::MissingValueRepresentation { .. }));
    }

    #[test]
    fn compiled_program_keeps_canonical_and_representation_as_one_success_value() {
        let program = typed_call_program();
        let caller = ItemInstanceId::from_index(0).unwrap();
        let argument = MirValueId::from_index(0).unwrap();
        let result = MirValueId::from_index(1).unwrap();
        let mut representation = RepresentationPlan::default();
        for value in [argument, result] {
            representation.value_reps.insert(
                ValueIdentity::new(caller, value),
                crate::semantic_ids::layout_choice::ValueRepresentation::Specialized,
            );
        }
        representation.invoke_lowerings.insert(InstructionId::from_index(0).unwrap(), crate::semantic_ids::layout_choice::InvokeLowering::Direct);
        let compiled = CompiledProgram::new(program.clone(), representation.clone()).expect("完整处理载荷必须成功");
        assert_eq!(compiled.canonical(), &program);
        assert_eq!(compiled.representation(), &representation);
    }

    #[test]
    fn compiled_program_rejects_representation_for_unknown_value() {
        let program = typed_call_program();
        let caller = ItemInstanceId::from_index(0).unwrap();
        let mut representation = RepresentationPlan::default();
        representation.value_reps.insert(
            ValueIdentity::new(caller, MirValueId::from_index(0).unwrap()),
            crate::semantic_ids::layout_choice::ValueRepresentation::Specialized,
        );
        representation.value_reps.insert(
            ValueIdentity::new(caller, MirValueId::from_index(1).unwrap()),
            crate::semantic_ids::layout_choice::ValueRepresentation::Specialized,
        );
        representation.value_reps.insert(
            ValueIdentity::new(caller, MirValueId::from_index(2).unwrap()),
            crate::semantic_ids::layout_choice::ValueRepresentation::Specialized,
        );
        representation.invoke_lowerings.insert(InstructionId::from_index(0).unwrap(), crate::semantic_ids::layout_choice::InvokeLowering::Direct);
        let error = CompiledProgram::new(program, representation).expect_err("未知值不得进入后端");
        assert!(matches!(error, CompiledProgramError::ExtraValueRepresentation { .. }));
    }

    #[test]
    fn compiled_program_rejects_representation_for_unknown_instruction() {
        let program = typed_call_program();
        let caller = ItemInstanceId::from_index(0).unwrap();
        let mut representation = RepresentationPlan::default();
        for value in [MirValueId::from_index(0).unwrap(), MirValueId::from_index(1).unwrap()] {
            representation.value_reps.insert(
                ValueIdentity::new(caller, value),
                crate::semantic_ids::layout_choice::ValueRepresentation::Specialized,
            );
        }
        representation.invoke_lowerings.insert(InstructionId::from_index(0).unwrap(), crate::semantic_ids::layout_choice::InvokeLowering::Direct);
        representation.invoke_lowerings.insert(InstructionId::from_index(1).unwrap(), crate::semantic_ids::layout_choice::InvokeLowering::Direct);
        let error = CompiledProgram::new(program, representation).expect_err("未知调用指令不得进入后端");
        assert!(matches!(error, CompiledProgramError::ExtraInvokeRepresentation { .. }));
    }

    #[test]
    fn canonical_call_rejects_argument_type_mismatch() {
        let mut program = typed_call_program();
        let other = TypeId::from_index(1).unwrap();
        program.linked.types.insert(other, TypeRecord { declaration: other, kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Bool) });
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

    fn typed_function_value_program() -> CanonicalProgram {
        let caller = ItemInstanceId::from_index(0).unwrap();
        let function_type = TypeId::from_index(0).unwrap();
        let value_type = TypeId::from_index(1).unwrap();
        let function_value = MirValueId::from_index(0).unwrap();
        let argument = MirValueId::from_index(1).unwrap();
        let result = MirValueId::from_index(2).unwrap();
        let mut linked = LinkedSemanticProgram::default();
        linked.types.insert(function_type, TypeRecord {
            declaration: function_type,
            kind: CanonicalTypeKind::Function { parameters: vec![value_type], return_type: value_type },
        });
        linked.types.insert(value_type, TypeRecord {
            declaration: value_type,
            kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { bits: 32, signed: true }),
        });
        linked.item_instances.insert(caller, ItemInstanceRecord {
            declaration: ItemId::from_index(0).unwrap(),
            substitution: SubstitutionId::from_index(0).unwrap(),
            parameter_types: vec![function_type, value_type],
            return_type: value_type,
        });
        let function = CanonicalFunction {
            instance: caller,
            parameters: vec![(function_value, function_type), (argument, value_type)],
            return_type: value_type,
            value_types: BTreeMap::from([(function_value, function_type), (argument, value_type), (result, value_type)]),
            entry: CanonicalBlockId(0),
            blocks: BTreeMap::from([(CanonicalBlockId(0), CanonicalBlock {
                id: CanonicalBlockId(0),
                parameters: Vec::new(),
                instructions: vec![CanonicalInstruction {
                    id: InstructionId::from_index(0).unwrap(),
                    results: vec![result],
                    operation: CanonicalOperation::Invoke { callee: CanonicalCallee::Value(function_value), arguments: vec![argument] },
                }],
                terminator: CanonicalTerminator::Return { value: Some(result) },
            })]),
        };
        CanonicalProgram {
            linked,
            mir: CanonicalSemanticMir { module_name: "function-value".into(), functions: BTreeMap::from([(caller, function)]) },
        }
    }

    #[test]
    fn canonical_function_value_call_preserves_function_type() {
        typed_function_value_program().validate().expect("函数值调用必须通过完整语义合同");
    }

    #[test]
    fn canonical_function_value_call_rejects_scalar_callee() {
        let mut program = typed_function_value_program();
        let function = program.mir.functions.values_mut().next().unwrap();
        let value = MirValueId::from_index(0).unwrap();
        function.value_types.insert(value, TypeId::from_index(1).unwrap());
        function.parameters[0].1 = TypeId::from_index(1).unwrap();
        program.linked.item_instances.get_mut(&ItemInstanceId::from_index(0).unwrap()).unwrap().parameter_types[0] = TypeId::from_index(1).unwrap();
        assert!(matches!(program.validate(), Err(CanonicalMirError::ValueCalleeNotFunction { value: rejected, .. }) if rejected == value));
    }

    #[test]
    fn canonical_function_value_call_rejects_argument_contract() {
        let mut program = typed_function_value_program();
        let wrong = MirValueId::from_index(0).unwrap();
        if let CanonicalOperation::Invoke { arguments, .. } = &mut program.mir.functions.values_mut().next().unwrap().blocks.get_mut(&CanonicalBlockId(0)).unwrap().instructions[0].operation {
            *arguments = vec![wrong, wrong];
        }
        assert!(matches!(program.validate(), Err(CanonicalMirError::ValueCalleeArityMismatch { .. })));
    }

    #[test]
    fn canonical_function_value_callee_is_an_ssa_use() {
        let mut program = typed_function_value_program();
        let function = program.mir.functions.values_mut().next().unwrap();
        function.parameters = vec![(MirValueId::from_index(1).unwrap(), TypeId::from_index(1).unwrap())];
        program.linked.item_instances.get_mut(&ItemInstanceId::from_index(0).unwrap()).unwrap().parameter_types = vec![TypeId::from_index(1).unwrap()];
        assert!(matches!(program.validate(), Err(CanonicalMirError::UseBeforeDefinition { value, .. }) if value == MirValueId::from_index(0).unwrap()));
    }

    #[test]
    fn stage_order_is_one_way() {
        assert_eq!(pipeline::STAGE_ORDER[0], CompileStage::Ast);
        assert_eq!(*pipeline::STAGE_ORDER.last().unwrap(), CompileStage::Emit);
    }
}
