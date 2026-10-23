//! Canonical Semantic MIR 结构与 M2 合同错误。
//!
//! 本模块拥有已解析、已类型化的 Semantic MIR：块、指令、操作、终结符与
//! effect 类别，以及验证失败的确定性原因。这里不引入 AST 运算符类别。

use crate::semantic_ids::{FieldId, InstructionId, ItemInstanceId, MirValueId, NominalInstanceId, TypeId, VariantId};
use std::collections::BTreeMap;

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
    Invoke {
        /// 已解析的调用目标。
        callee: CanonicalCallee,
        /// 按签名顺序排列的实参 SSA 值。
        arguments: Vec<MirValueId>,
    },
    /// SSA 值复制。
    Copy {
        /// 被复制的源 SSA 值。
        source: MirValueId,
    },
    /// 语言层类型转换；结果 TypeId 必须等于 `ty`。
    Cast {
        /// 被转换的源 SSA 值。
        source: MirValueId,
        /// 转换后的目标类型。
        ty: TypeId,
    },
    /// 将 SSA 值绑定到命名局部（`let`）。
    StoreVar {
        /// 局部变量名（诊断与 deopt 用）。
        name: String,
        /// 绑定的 SSA 值。
        value: MirValueId,
        /// 可选的显式类型注解。
        ty: Option<TypeId>,
    },
    /// 按已解析聚合类型复制值。
    AggregateCopy {
        /// 源聚合 SSA 值。
        source: MirValueId,
        /// 目标聚合 SSA 值。
        destination: MirValueId,
    },
    /// 加载语言常量。
    LoadConstant {
        /// 已类型化的常量载荷。
        constant: CanonicalConstant,
    },
    /// 构造已解析的 sum variant。
    SumNew {
        /// 目标 sum 的名义实例。
        nominal: NominalInstanceId,
        /// 要构造的 variant。
        variant: VariantId,
        /// 可选的 variant payload。
        payload: Option<MirValueId>,
    },
    /// 提取已解析 variant 的 payload。
    SumPayloadGet {
        /// sum 的名义实例。
        nominal: NominalInstanceId,
        /// 要提取的 variant。
        variant: VariantId,
        /// sum 对象 SSA 值。
        object: MirValueId,
    },
    /// 检查 sum 值是否为已解析 variant。
    SumVariantIs {
        /// sum 的名义实例。
        nominal: NominalInstanceId,
        /// 要检测的 variant。
        variant: VariantId,
        /// sum 对象 SSA 值。
        object: MirValueId,
    },
    /// 构造名义聚合。
    StructNew {
        /// 目标结构体的名义实例。
        nominal: NominalInstanceId,
        /// 字段身份与对应 SSA 值。
        fields: Vec<(FieldId, MirValueId)>,
    },
    /// 读取已解析字段。
    FieldGet {
        /// 聚合对象 SSA 值。
        object: MirValueId,
        /// 要读取的字段身份。
        field: FieldId,
    },
    /// 写入已解析字段。
    FieldSet {
        /// 聚合对象 SSA 值。
        object: MirValueId,
        /// 要写入的字段身份。
        field: FieldId,
        /// 新字段值。
        value: MirValueId,
    },
    /// 从完整类型的数组读取。
    ArrayGet {
        /// 数组 SSA 值。
        array: MirValueId,
        /// 下标 SSA 值。
        index: MirValueId,
    },
    /// 按完整类型和语言初始化规则构造数组。
    ArrayNew {
        /// 完整数组类型（含元素与长度合同）。
        array_type: TypeId,
        /// 运行时长度 SSA 值。
        length: MirValueId,
        /// 元素初始化策略。
        initialization: CanonicalArrayInitialization,
    },
    /// 从完整元素序列构造数组。
    ArrayFromElements {
        /// 完整数组类型。
        array_type: TypeId,
        /// 按求值顺序排列的元素 SSA 值。
        elements: Vec<MirValueId>,
    },
    /// 向完整类型的数组写入。
    ArraySet {
        /// 数组 SSA 值。
        array: MirValueId,
        /// 下标 SSA 值。
        index: MirValueId,
        /// 要写入的元素值。
        value: MirValueId,
    },
    /// 读取数组长度。
    ArrayLength {
        /// 数组 SSA 值。
        array: MirValueId,
    },
    /// 向可增长数组末尾追加元素。
    ArrayPush {
        /// 可增长数组 SSA 值。
        array: MirValueId,
        /// 要追加的元素值。
        value: MirValueId,
    },
    /// 检测可空值为 null。
    NullIsNull {
        /// 可空值 SSA 值。
        value: MirValueId,
    },
    /// 解包非 null 可空载荷。
    NullUnwrap {
        /// 已确认为非 null 的可空值。
        value: MirValueId,
    },
    /// 构造元组值。
    TupleNew {
        /// 各字段的完整类型列表。
        element_types: Vec<TypeId>,
        /// 与类型列表对齐的字段 SSA 值。
        fields: Vec<MirValueId>,
    },
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
    Return {
        /// 返回值 SSA 值；`None` 表示 void 返回。
        value: Option<MirValueId>,
    },
    /// 带显式并行块参数的跳转。
    Jump {
        /// 目标基本块。
        target: CanonicalBlockId,
        /// 传给目标块参数的 SSA 值。
        arguments: Vec<MirValueId>,
    },
    /// 条件分支。
    Branch {
        /// 分支条件 SSA 值。
        condition: MirValueId,
        /// 条件为真时的目标块。
        then_target: CanonicalBlockId,
        /// 条件为假时的目标块。
        else_target: CanonicalBlockId,
    },
    /// 执行已解析 effect，并转移到恢复块。
    PerformEffect {
        /// effect 类别。
        effect: CanonicalEffectKind,
        /// 可选的 effect 载荷。
        payload: Option<MirValueId>,
        /// effect 完成后的恢复块。
        resume_target: CanonicalBlockId,
    },
    /// 按已解析状态值分发到恢复块。
    StateDispatch {
        /// 分发用的整数状态 SSA 值。
        state: MirValueId,
        /// `(状态值, 目标块)` 映射表。
        cases: Vec<(u32, CanonicalBlockId)>,
        /// 无匹配状态时的默认目标块。
        default_target: CanonicalBlockId,
    },
    /// 将 effect 控制权交还 runtime。
    YieldToRuntime {
        /// effect 类别。
        effect: CanonicalEffectKind,
        /// 可选的 effect 载荷。
        payload: Option<MirValueId>,
        /// runtime 恢复时使用的状态编号。
        resume_state: u32,
    },
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
    MissingNominalSemantics {
        /// 缺少语义的名义声明类型。
        declaration: TypeId,
    },
    /// 值类型没有对应完整 TypeId 的名义实例合同。
    MissingNominalInstance {
        /// 缺少名义实例合同的完整类型。
        ty: TypeId,
    },
    /// 名义实例没有对应的名义声明类型。
    InvalidNominalDeclaration {
        /// 无效的名义实例。
        nominal: NominalInstanceId,
        /// 引用的声明类型。
        declaration: TypeId,
    },
    /// 实例类型未知、非名义类型或不属于该声明。
    InvalidNominalInstanceType {
        /// 名义实例身份。
        nominal: NominalInstanceId,
        /// 不匹配的完整类型。
        ty: TypeId,
    },
    /// 同一完整实例类型被重复登记为不同名义实例。
    DuplicateNominalInstanceType {
        /// 被重复登记的完整类型。
        ty: TypeId,
    },
    /// 同一声明的实例携带了互相矛盾的值/引用语义。
    ConflictingNominalSemantics {
        /// 发生冲突的名义声明类型。
        declaration: TypeId,
    },
    /// 字段清单重复、缺失或与 owner/类型合同不一致。
    InvalidNominalField {
        /// 名义实例 owner。
        nominal: NominalInstanceId,
        /// 无效的字段身份。
        field: FieldId,
    },
    /// 公开导出或程序入口未指向本闭包中的函数体。
    MissingSurfaceBody {
        /// 缺少函数体的导出项实例。
        function: ItemInstanceId,
    },
    /// 不同 callable 使用了同一公开导出名。
    DuplicateExportName {
        /// 重复的导出名。
        name: String,
    },
    /// effect 终结符缺少语义 payload。
    MissingEffectPayload {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 所在基本块。
        block: CanonicalBlockId,
    },
    /// 状态分发值不是整数类型。
    InvalidDispatchState {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 非法的状态 SSA 值。
        value: MirValueId,
    },
    /// 状态分发中同一状态存在多个目标。
    DuplicateDispatchState {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 所在基本块。
        block: CanonicalBlockId,
        /// 重复的状态值。
        state: u32,
    },
    /// 聚合实例身份未知。
    UnknownNominal {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 未知的名义实例。
        nominal: NominalInstanceId,
    },
    /// 外部导入记录与 callable 签名不一致。
    ImportSignatureMismatch {
        /// 签名不一致的函数实例。
        function: ItemInstanceId,
    },
    /// 类型形状引用了不存在的类型。
    UnknownTypeReference {
        /// 引用方的类型 owner。
        owner: TypeId,
        /// 被引用但未知的类型。
        referenced: TypeId,
    },
    /// 原生标量宽度不属于语言类型合同。
    InvalidPrimitiveType {
        /// 非法原生标量类型。
        ty: TypeId,
    },
    /// 表键与函数内部实例身份不一致。
    FunctionKeyMismatch {
        /// 函数表键。
        key: ItemInstanceId,
        /// 函数记录中的实例身份。
        instance: ItemInstanceId,
    },
    /// Semantic MIR 函数自身不在链接闭包中。
    UnknownFunction {
        /// 未知的函数实例。
        function: ItemInstanceId,
    },
    /// 函数引用了未链接的 callee。
    UnknownCallee {
        /// 调用方函数实例。
        function: ItemInstanceId,
        /// 未链接的 callee 实例。
        callee: ItemInstanceId,
    },
    /// 类型身份未进入 canonical type table。
    UnknownType {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 未知的类型身份。
        ty: TypeId,
    },
    /// SSA 值在定义前被使用。
    UseBeforeDefinition {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 未定义的 SSA 值。
        value: MirValueId,
    },
    /// 一个 SSA 值被重复定义。
    DuplicateDefinition {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 被重复定义的 SSA 值。
        value: MirValueId,
    },
    /// 调用实参数量与实例化签名不一致。
    CallArityMismatch {
        /// 调用方函数实例。
        function: ItemInstanceId,
        /// 被调用的项实例。
        callee: ItemInstanceId,
    },
    /// 调用实参的 SSA 类型与实例化签名不一致。
    CallArgumentTypeMismatch {
        /// 调用方函数实例。
        function: ItemInstanceId,
        /// 被调用的项实例。
        callee: ItemInstanceId,
        /// 类型不匹配的实参 SSA 值。
        value: MirValueId,
    },
    /// 调用结果的 SSA 类型与实例化返回类型不一致。
    CallResultTypeMismatch {
        /// 调用方函数实例。
        function: ItemInstanceId,
        /// 被调用的项实例。
        callee: ItemInstanceId,
        /// 类型不匹配的结果 SSA 值。
        value: MirValueId,
    },
    /// 函数值 callee 的 SSA 类型不是函数类型。
    ValueCalleeNotFunction {
        /// 调用方函数实例。
        function: ItemInstanceId,
        /// 非函数类型的 callee SSA 值。
        value: MirValueId,
    },
    /// 函数值 callee 的实参数量不一致。
    ValueCalleeArityMismatch {
        /// 调用方函数实例。
        function: ItemInstanceId,
        /// 函数值 callee 的 SSA 值。
        value: MirValueId,
    },
    /// 函数值 callee 的实参类型不一致。
    ValueCalleeArgumentTypeMismatch {
        /// 调用方函数实例。
        function: ItemInstanceId,
        /// 函数值 callee 的 SSA 值。
        value: MirValueId,
        /// 类型不匹配的实参 SSA 值。
        argument: MirValueId,
    },
    /// 函数值 callee 的结果类型不一致。
    ValueCalleeResultTypeMismatch {
        /// 调用方函数实例。
        function: ItemInstanceId,
        /// 函数值 callee 的 SSA 值。
        value: MirValueId,
        /// 类型不匹配的结果 SSA 值。
        result: MirValueId,
    },
    /// 函数参数的 SSA 类型表记录不一致。
    ValueTypeMismatch {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 类型表记录不一致的 SSA 值。
        value: MirValueId,
        /// 期望的类型。
        expected: TypeId,
    },
    /// 函数入口或返回合同与其实例化声明不一致。
    FunctionSignatureMismatch {
        /// 签名不一致的函数实例。
        function: ItemInstanceId,
    },
    /// 返回值类型与函数合同不一致。
    ReturnTypeMismatch {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 类型不匹配的返回值 SSA 值。
        value: MirValueId,
    },
    /// 基本块引用了不存在的目标。
    UnknownBlock {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 未知的基本块身份。
        block: CanonicalBlockId,
    },
    /// 跳转参数数量与目标块参数不一致。
    BlockParameterArityMismatch {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 参数数量不匹配的目标块。
        block: CanonicalBlockId,
    },
    /// 块参数或指令结果重复定义 SSA 值。
    DuplicateBlockDefinition {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 被重复定义的 SSA 值。
        value: MirValueId,
    },
    /// 终结符使用了未定义的 SSA 值。
    TerminatorUseBeforeDefinition {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 未定义的 SSA 值。
        value: MirValueId,
    },
    /// SSA 定义块不支配使用块。
    NonDominatingUse {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 被非法使用的 SSA 值。
        value: MirValueId,
        /// 发生使用的基本块。
        block: CanonicalBlockId,
    },
    /// 跳转实参与目标块参数类型不一致。
    BlockParameterTypeMismatch {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 目标基本块。
        block: CanonicalBlockId,
        /// 类型不匹配的块参数 SSA 值。
        value: MirValueId,
    },
    /// 聚合字段身份未知。
    UnknownField {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 未知的字段身份。
        field: FieldId,
    },
    /// 字段不属于当前聚合 owner。
    FieldOwnerMismatch {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 字段身份。
        field: FieldId,
        /// 声明的 owner 名义实例。
        nominal: NominalInstanceId,
    },
    /// 字段值类型与声明不一致。
    FieldTypeMismatch {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 字段身份。
        field: FieldId,
        /// 类型不匹配的字段 SSA 值。
        value: MirValueId,
    },
    /// 聚合构造缺少声明字段。
    MissingStructField {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 结构体名义实例。
        nominal: NominalInstanceId,
        /// 缺失的字段身份。
        field: FieldId,
    },
    /// 聚合构造重复声明字段。
    DuplicateStructField {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 结构体名义实例。
        nominal: NominalInstanceId,
        /// 重复的字段身份。
        field: FieldId,
    },
    /// sum variant 身份未知。
    UnknownVariant {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 未知的 variant 身份。
        variant: VariantId,
    },
    /// sum variant 的具体名义 owner 不存在。
    InvalidVariantOwner {
        /// 名义实例 owner。
        nominal: NominalInstanceId,
        /// 无效的 variant 身份。
        variant: VariantId,
    },
    /// sum variant payload 与声明合同不一致。
    VariantPayloadMismatch {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 合同不一致的 variant 身份。
        variant: VariantId,
    },
    /// 结构操作的结果类型与其声明合同不一致。
    OperationResultTypeMismatch {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 结果类型不一致的指令。
        instruction: InstructionId,
    },
    /// 结构操作的输入类型与其声明合同不一致。
    OperationOperandTypeMismatch {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 操作数类型不一致的指令。
        instruction: InstructionId,
        /// 类型不匹配的操作数 SSA 值。
        value: MirValueId,
    },
    /// 结构操作的结果个数与其语义合同不一致。
    OperationResultArityMismatch {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 结果个数不一致的指令。
        instruction: InstructionId,
    },
    /// 结构操作的输入个数与其类型合同不一致。
    OperationOperandArityMismatch {
        /// 所在函数实例。
        function: ItemInstanceId,
        /// 操作数个数不一致的指令。
        instruction: InstructionId,
    },
}
