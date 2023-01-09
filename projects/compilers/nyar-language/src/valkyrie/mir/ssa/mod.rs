#![allow(missing_docs)]

use std::collections::BTreeMap;

use crate::{
    symbols::stable_hir_function_symbol,
    types::{
        Identifier, NamePath,
        hir::{HirExpr, HirFunction, HirImpl, HirMatchArm, HirModule, HirPattern, HirStatement, HirStatementKind, ValkyrieType},
    },
};

pub(crate) mod builtin_helpers;
mod control_flow_context;
mod control_flow_lowering;
mod effect_lowering;
mod exit_lowering;
mod expr_helpers;
mod expr_lowering;
// DELETED ADR0011: mod frame_planning;
mod match_lowering;
mod pattern_lowering;
// DELETED ADR0011: pub mod state_machine_cfg_rewrite;
// DELETED ADR0011: mod suspend_analysis;
/// `MIR` 单元测试辅助工具（构造 builder / 断言结构）。
pub mod test_support;
mod try_propagate_lowering;
mod try_scope_lowering;
mod value_semantics;

#[cfg(test)]
mod singleton_tests;

// ADR 0010: IntrinsicOpcode authority deleted — do not `pub use` opcode enums.
pub use value_semantics::{
    AggregateLayout, AggregateLayoutPlan, FieldLayout, FlagsLayout, LayoutId, MirStorageKind, SumTypeLayout, SumVariantLayout,
    compute_aggregate_layout_plan, ensure_layout_for_type, ensure_named_aggregate_layout, layout_id_for_nyar_type, layout_id_for_type,
    layout_key_for_nyar_type, layout_key_for_type, merge_aggregate_layout_plan, storage_kind_for_named_type, storage_kind_for_type,
    value_type_names_from_module,
};

use builtin_helpers::plain_type_pattern_matches;
use control_flow_context::{MirBuilderControlFlow, MirHandlerDispatchContext, MirResumeContinuationContext};
use expr_helpers::{callee_name_matches, future_resume_type, infer_builder_operand_type, lower_callee_operand, named_type_name};
use expr_lowering::lower_literal;

/// `MIR` lowering 阶段产生的编译期诊断。
///
/// 当 lowering 过程中遇到可在编译期判定为无法处理的构造时，
/// 会向模块中追加一条诊断，最终由校验层转化为编译错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirDiagnostic {
    /// 模式匹配无法 lowering 到具体 `MIR` 指令。
    ///
    /// 触发场景包括：extractor 未 resolved、scrutinee 类型未知导致无法选择
    /// probe 策略、tuple/object 类型推断失败等。用户需补类型注解或 extractor
    /// 定义后才能继续 lowering。
    PatternLoweringFailed {
        /// 触发诊断的模式。
        pattern: HirPattern,
        /// 诊断原因描述。
        reason: String,
    },
    UnsupportedExpression {
        span: crate::SourceSpan,
        kind: String,
    },
}

/// `SSA` 形式的 `MIR` 模块。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirModule {
    pub name: String,
    pub functions: Vec<MirFunction>,
    /// 结构体定义，保留字段类型供后端继续推导 `TypeDef` / `Field` 信息。
    pub structs: Vec<MirStruct>,
    /// `using` 导入列表，记录模块依赖的外部命名空间与类型别名。
    pub imports: Vec<String>,
    /// Exact call contracts exported by resolved dependency modules.
    ///
    /// This is deliberately not derived from [`Self::imports`]: an import
    /// path proves neither that a symbol exists nor its call signature.
    /// Backend preparation may consume these contracts, but must not create
    /// new language semantics from its host ABI.
    pub external_calls: Vec<MirExternalCallContract>,
    /// Value/reference aggregate inline layout plan.
    pub aggregate_layouts: value_semantics::AggregateLayoutPlan,
    /// Canonical nominal-sum registry.  This is language semantic metadata:
    /// nullable `T?` is represented by `ValkyrieType::Nullable`, while a
    /// declared sum such as `Option<T>` is represented here with its tags and
    /// payload contracts.  Backends may project either form differently, but
    /// must not reconstruct either from names or host string representations.
    pub sum_types: Vec<SumTypeLayout>,
    /// `MIR` lowering 过程中收集的编译期诊断，由校验层转化为编译错误。
    pub diagnostics: Vec<MirDiagnostic>,
}

/// A statically resolved call contract owned by an imported semantic export.
///
/// ADR 0010: no `dispatch` / witness / intrinsic side-channels. Types belong on
/// ItemInstance / type table once Invoke lands — not reattached here as God fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirExternalCallContract {
    /// Exact source-level symbol selected by HIR overload resolution.
    pub symbol: NamePath,
}

/// `MIR` 结构体定义。
/// 保留字段类型，供后端生成 `TypeDef`/`Field` 并推导布局。
/// `is_value_type` 区分值类型聚合与引用类型聚合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirStruct {
    /// 结构体名称。
    pub name: String,
    /// 结构体命名空间（点分路径，例如 `core.text`），用于稳定符号与布局查找。
    pub namespace: String,
    /// 字段列表。
    pub fields: Vec<MirField>,
    /// 是否为值类型：`true` 表示 `structure` 等按值布局的聚合；
    /// `false` 表示引用类型，例如 `class` 等按引用布局的聚合。
    pub is_value_type: bool,
}

/// `MIR` 结构体字段定义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirField {
    /// 字段名。
    pub name: String,
    /// 字段静态类型。
    pub ty: ValkyrieType,
}

/// `SSA` 形式的 `MIR` 函数定义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirFunction {
    pub symbol: String,
    /// 函数返回类型，用于后端判断调用是否返回 `void`。
    pub return_type: ValkyrieType,
    /// 函数参数类型列表，用于后端生成调用约定与方法签名。
    pub param_types: Vec<ValkyrieType>,
    /// `SSA` 值的静态类型表，供调度校验与后续 lowering 使用。
    pub value_types: BTreeMap<MirValueRef, ValkyrieType>,
    pub entry: MirBlockRef,
    pub values: Vec<MirValue>,
    pub blocks: Vec<MirBlock>,
}

/// `MIR` 基本块。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirBlock {
    pub id: MirBlockRef,
    pub label: String,
    pub parameters: Vec<MirValueRef>,
    pub instructions: Vec<MirInstruction>,
    pub terminator: MirTerminator,
}

/// `MIR` SSA 值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirValue {
    pub id: MirValueRef,
    pub origin: MirValueOrigin,
}

/// `catch / resume` 上下文中下推的 continuation 元数据，承载 resume 跳转与类型信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirContinuation {
    /// 该 continuation 对应的 handler dispatch block。
    pub dispatch_block: MirBlockRef,
    /// Continuation 恢复时跳转的 resume block。
    pub resume_target: MirBlockRef,
    /// Resume block 入口参数对应的 SSA 值。
    pub resume_parameter: MirValueRef,
    /// Resume 参数静态类型，未确定时为 `None`。
    pub resume_parameter_type: Option<ValkyrieType>,
    /// Handler 退出块。
    pub handler_exit: MirBlockRef,
    /// 触发该 continuation 的用户 effect 载体类型。
    ///
    /// 仅在 `MirEffectKind::Raise` 且被 raise 值具备可识别的载体类型时填入；
    /// 用于查询 `Effectful::Resume` 关联类型。其他 effect 或纯异常路径保持 `None`。
    pub carrier_type: Option<ValkyrieType>,
}

/// `match / case` 链路元数据，描述分发、汇合与各 arm 结构。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirCaseChain {
    /// 进入 case-like lowering 的分发块。
    pub dispatch_block: MirBlockRef,
    /// 第一条 arm 的入口块。
    pub first_arm: MirBlockRef,
    /// 全部 arm 未匹配时跳入的块。
    pub no_match_block: MirBlockRef,
    /// Case-like 控制流最终汇入的 exit 块。
    pub exit_block: MirBlockRef,
    /// 是否为值语义 `match`。
    pub produce_value: bool,
    /// 各个 arm 的显式链路信息。
    pub arms: Vec<MirCaseArm>,
}

/// 单个 `case / match arm` 的控制流元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirCaseArm {
    /// Arm 入口块。
    pub entry_block: MirBlockRef,
    /// 可选的 pattern check 块。
    pub check_block: Option<MirBlockRef>,
    /// 可选的 guard 求值块。
    pub guard_block: Option<MirBlockRef>,
    /// Arm body 实际执行块。
    pub body_block: MirBlockRef,
    /// Pattern / guard 失败后跳向的下一目标。
    pub next_arm_target: MirBlockRef,
    /// 正常完成后汇入的 exit 目标。
    pub exit_target: MirBlockRef,
    /// `fallthrough` 允许时应跳入的下一 arm 入口。
    pub fallthrough_target: Option<MirBlockRef>,
}

/// `yield / await / block / raise` 等 suspend 点的元数据，记录状态机改写所需的全部信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirSuspendPoint {
    /// 状态机编号，由 lowering 分配且全局唯一。
    pub state_id: u32,
    /// 该挂起点对应的 effect 类别。
    pub effect: MirEffectKind,
    /// 挂起所在 block。
    pub suspend_block: MirBlockRef,
    /// 恢复时跳转到的 block。
    pub resume_target: MirBlockRef,
    /// 恢复参数数量。
    pub resume_parameter_count: usize,
    /// 触发该挂起点的 payload 静态类型（即被 raise/yield 的值的类型）。
    pub payload_type: Option<ValkyrieType>,
    /// 跨挂起点需要 spill 到 frame 的 SSA 值。
    pub spill_candidates: Vec<MirValueRef>,
    /// 若该挂起点位于 catch arm body 内，对应 continuation 的索引。
    pub continuation_index: Option<usize>,
    /// 触发该挂起点的用户 effect 载体类型。
    ///
    /// 仅 `MirEffectKind::Raise` 且被 raise 值具备可识别载体类型时填入；
    /// 用于查询 `Effectful::Resume` 关联类型以推断 resume 参数类型。
    /// 其他 effect 类别或纯异常路径保持 `None`。
    pub carrier_type: Option<ValkyrieType>,
}

/// 单个 suspend 点对应的 frame layout，描述跨挂起点需要保留的槽位。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirFrameLayout {
    /// 该 layout 所属的状态机编号。
    pub state_id: u32,
    /// 该 layout 所属的 effect 类别。
    pub effect: MirEffectKind,
    /// 恢复时跳转到的 block。
    pub resume_target: MirBlockRef,
    /// Frame 中需要保留的 spill 槽位。
    pub slots: Vec<MirFrameSlot>,
    /// 触发该 frame 的用户 effect 载体类型（仅 `Raise` 时填入）。
    pub carrier_type: Option<ValkyrieType>,
}

/// Frame 中的单个 spill 槽位。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirFrameSlot {
    /// Frame 中的稳定槽位序号。
    pub slot_index: usize,
    /// 被保存的 SSA 值。
    pub value: MirValueRef,
    /// 当前已知的槽位静态类型。
    pub value_type: Option<ValkyrieType>,
}

/// `MIR` SSA 值引用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MirValueRef(pub u32);

/// `MIR` 基本块引用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MirBlockRef(pub u32);

/// `MIR` 值的来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirValueOrigin {
    Parameter {
        index: usize,
        name: String,
    },
    BlockParameter {
        block: MirBlockRef,
        name: String,
    },
    LetBinding {
        name: String,
    },
    /// Pattern `mut pat`：可变借用绑定（类似 Rust `ref mut`）。
    MutRefBinding {
        name: String,
    },
    /// Pattern `pin mut pat`：固定的可变借用绑定。
    PinMutRefBinding {
        name: String,
    },
    Literal,
    Path,
    CallResult,
    Temporary,
}

/// `MIR` 操作数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirOperand {
    Value(MirValueRef),
    Constant(MirConstant),
    Symbol(NamePath),
}

/// `MIR` 常量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirConstant {
    Int(i64),
    Float64(ordered_float::OrderedFloat<f64>),
    Bool(bool),
    /// UTF-8 text literal. The language has no encoding-neutral string type.
    Utf8(String),
    /// UTF-16 text literal. This is a language-level encoding tag, not a
    /// request to use any backend's default string representation.
    Utf16(String),
    Unit,
}

/// `MIR` effect terminator category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirEffectKind {
    /// `raise expr`
    Raise,
    /// `yield expr`
    Yield,
    /// `yield from expr`
    DelegateYield,
    /// `expr.await`
    Await,
    /// `expr.awake`
    AsyncSpawn,
    /// `expr.block`
    AsyncBlock,
}

/// `MIR` 指令信封（ADR 0012）。
///
/// Field `kind` is a transitional name; semantic role is **operation**.
/// Result types live only in the value table (`MirValueDefinition`), not on the envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirInstruction {
    /// Stable instruction identity (plan / verifier key).
    pub id: nyar_types::InstructionId,
    /// SSA results produced by this instruction (0..n).
    pub results: Vec<MirValueRef>,
    /// Operation payload (`MirOperation` only — no result types / layouts here).
    pub kind: MirOperation,
    /// Source or synthetic provenance.
    pub provenance: nyar_types::ProvenanceId,
}

impl MirInstruction {
    /// Borrow the operation payload (ADR 0012 name).
    pub fn operation(&self) -> &MirOperation {
        &self.kind
    }

    /// Wrap an operation in a synthetic envelope (tests / transitional sites).
    ///
    /// Prefer [`MirBuilder::push_instruction`] so InstructionId is dense and unique.
    pub fn from_operation(operation: MirOperation) -> Self {
        Self {
            id: nyar_types::InstructionId::from_index(0).expect("stub InstructionId"),
            results: Vec::new(),
            kind: operation,
            provenance: nyar_types::ProvenanceId::from_index(0).expect("synthetic ProvenanceId"),
        }
    }

    /// Wrap an operation that produces SSA results.
    pub fn from_operation_with_results(operation: MirOperation, results: Vec<MirValueRef>) -> Self {
        Self {
            id: nyar_types::InstructionId::from_index(0).expect("stub InstructionId"),
            results,
            kind: operation,
            provenance: nyar_types::ProvenanceId::from_index(0).expect("synthetic ProvenanceId"),
        }
    }
}

/// `MIR` 指令种类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirOperation {
    LoadConstant {
        constant: MirConstant,
        ty: Option<ValkyrieType>,
    },
    LoadSymbol {
        path: NamePath,
    },
    Copy {
        source: MirOperand,
    },
    /// 将值写入具名局部变量（`let` 绑定）。
    ///
    /// `name` 为变量名，`value` 为待写入的操作数。
    /// `ty` 为可选静态类型，供后端槽位分配与调试信息使用；
    /// 若缺失，后端可回退到操作数已推断类型或 `header` 中的类型线索。
    StoreVar {
        name: String,
        value: MirOperand,
        /// 可选静态类型；来自 `let` 注解时优先使用该 `ty`。
        ty: Option<ValkyrieType>,
    },
    Call {
        /// Callable operand. Transitional: will become `Invoke.callee` (`Item` | `Value`).
        /// Must not carry dispatch / witness / intrinsic / generic side-channels (ADR 0010).
        callee: MirOperand,
        /// Semantic arguments (receiver is explicit arg 0 when present).
        arguments: Vec<MirOperand>,
    },
    /// Construct a named aggregate. Physical StorageKind/LayoutId belong in RepresentationPlan (ADR 0011).
    StructNew {
        type_name: String,
        fields: Vec<(String, MirOperand)>,
    },
    TupleNew {
        fields: Vec<MirOperand>,
    },
    AggregateCopy {
        source: MirOperand,
        dest: MirOperand,
    },
    FieldGet {
        object: MirOperand,
        field: String,
    },
    FieldSet {
        object: MirOperand,
        field: String,
        value: MirOperand,
    },
    /// Construct a value of an explicitly declared nominal sum variant.
    SumNew {
        sum_type: String,
        /// Type arguments with `sum_type` form NominalInstanceKey (empty ⇒ monomorphic).
        type_args: Vec<ValkyrieType>,
        variant: String,
        payload_type: Option<ValkyrieType>,
        payload: Option<MirOperand>,
    },
    /// Extract a payload from an explicitly identified nominal sum variant.
    /// This must not be represented as a field named `value`, `error`, or
    /// `payload`: those spellings carry no language semantics by themselves.
    SumPayloadGet {
        sum_type: String,
        /// Type arguments with `sum_type` form NominalInstanceKey (empty ⇒ monomorphic).
        type_args: Vec<ValkyrieType>,
        variant: String,
        payload_type: ValkyrieType,
        object: MirOperand,
    },
    /// Test whether a sum value is the given declared variant (`case Variant:`).
    /// Carries the same NominalInstanceKey as [`Self::SumNew`] / [`Self::SumPayloadGet`].
    SumVariantIs {
        sum_type: String,
        type_args: Vec<ValkyrieType>,
        variant: String,
        object: MirOperand,
    },
    /// 模式探测指令，用于 handler/case dispatch 的匹配检查。
    PatternMatch {
        /// 被匹配的值（scrutinee）。
        value: MirOperand,
        /// 待检查的模式。
        pattern: HirPattern,
    },
    /// Construct a language array by runtime length + prescribed initialization (ADR 0011).
    ///
    /// Fixed vs runtime-length is expressed by `array_type` (full array type / transitional
    /// stand-in for `TypeId`), **not** by a separate instruction. Do not invent
    /// `Uninitialized` unless valkyrie-2020 defines a verifiable unobservable init phase.
    /// Physical heap/inline/GC carrier belongs in RepresentationPlan — never here.
    ArrayNew {
        /// Full array type (`Array<T>` / `FixedArray<T,N>` …), not element-only.
        array_type: ValkyrieType,
        length: MirOperand,
        initialization: ArrayInitialization,
    },
    /// Construct a language array from a complete element sequence (ADR 0011).
    ///
    /// Replaces the former `FixedArrayNew` + `ArrayLiteral` God split (type-class ×
    /// init-style × surface syntax). Source “literal” is not a Semantic MIR category.
    /// For fixed arrays, M2 checks `elements.len()` against the length in `array_type`.
    ArrayFromElements {
        /// Full array type; element type and fixed length come from the type table.
        array_type: ValkyrieType,
        elements: Vec<MirOperand>,
    },
    ArrayGet {
        array: MirOperand,
        index: MirOperand,
    },
    ArraySet {
        array: MirOperand,
        index: MirOperand,
        value: MirOperand,
    },
    ArrayLength {
        array: MirOperand,
    },
}

/// How [`MirOperation::ArrayNew`] initializes slots (must be defined by valkyrie-2020).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrayInitialization {
    /// Language-defined default for the element type (not host CLR/JVM null invent).
    Default,
    /// Fill every slot with this value (evaluation order is CFG/SSA, not this enum).
    Fill(MirOperand),
}

/// `MIR` 基本块终结符。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirTerminator {
    Return {
        value: Option<MirOperand>,
    },
    Jump {
        target: MirBlockRef,
        arguments: Vec<MirOperand>,
    },
    Branch {
        condition: MirOperand,
        then_target: MirBlockRef,
        else_target: MirBlockRef,
    },
    PerformEffect {
        effect: MirEffectKind,
        payload: Option<MirOperand>,
        resume_target: MirBlockRef,
    },
    /// 状态机分发：按 `switch state` 跳转到 resume / entry 目标。
    StateDispatch {
        state: MirValueRef,
        cases: Vec<(u32, MirBlockRef)>,
        default_target: MirBlockRef,
    },
    /// 将控制权交还 runtime，并携带 `resume_state` 供后续 dispatch 恢复。
    YieldToRuntime {
        effect: MirEffectKind,
        payload: Option<MirOperand>,
        resume_state: u32,
    },
    Unreachable,
}

pub struct MirLowerer;

/// 收集模块中所有 `imply T: Effectful { type Resume = X }` 实现，构造载体类型名到 `Resume` 类型的映射。
///
/// 仅匹配 `trait_path` 最后一段为 `Effectful` 的 impl 块。载体类型按其 nominal 名提取：
/// - `ValkyrieType::Named(name)` → 直接使用 `name`
/// - `ValkyrieType::Apply(base, _)` → 取 `base` 的 nominal 名
///
/// 若同一载体存在多个 impl，后注册者覆盖前者。`Resume` 关联类型未声明的 impl 不产生条目。
pub(super) fn collect_effectful_resume_map(module: &HirModule) -> BTreeMap<String, ValkyrieType> {
    let mut map: BTreeMap<String, ValkyrieType> = BTreeMap::new();
    for impl_block in &module.impls {
        if !impl_block.trait_path.as_ref().is_some_and(|path| path.name().as_str() == "Effectful") {
            continue;
        }
        let Some(carrier_name) = nominal_type_name(&impl_block.target)
        else {
            continue;
        };
        if let Some(resume_impl) = impl_block.associated_type_impls.iter().find(|item| item.name.as_str() == "Resume") {
            map.insert(carrier_name.to_string(), resume_impl.concrete_type.clone());
        }
    }
    map
}

/// 提取 `ValkyrieType` 的 nominal 名（用于匹配 effect 载体与 `Effectful` impl 的 target）。
pub(super) fn nominal_type_name(ty: &ValkyrieType) -> Option<&str> {
    match ty {
        ValkyrieType::Named(name) => Some(name.as_str()),
        ValkyrieType::Apply(base, _) => nominal_type_name(base),
        ValkyrieType::Integer8 { signed: true } => Some("i8"),
        ValkyrieType::Integer8 { signed: false } => Some("u8"),
        ValkyrieType::Integer16 { signed: true } => Some("i16"),
        ValkyrieType::Integer16 { signed: false } => Some("u16"),
        ValkyrieType::Integer32 { signed: true } => Some("i32"),
        ValkyrieType::Integer32 { signed: false } => Some("u32"),
        ValkyrieType::Integer64 { signed: true } => Some("i64"),
        ValkyrieType::Integer64 { signed: false } => Some("u64"),
        ValkyrieType::Integer128 { signed: true } => Some("i128"),
        ValkyrieType::Integer128 { signed: false } => Some("u128"),
        _ => None,
    }
}

impl MirLowerer {
    /// Lowers HIR to semantic MIR (PerformEffect intact, no state-machine rewrite).
    pub fn lower_module_semantic(module: &HirModule) -> MirModule {
        eprintln!(
            "[seed-debug] mir-lower-start module={} hir_functions={} structs={} impls={}",
            module.name,
            module.functions.len(),
            module.structs.len(),
            module.impls.len()
        );
        let mut return_types = collect_module_return_types(module);
        return_types.extend(crate::valkyrie::mir::collect_singleton_return_types(module));
        let mut struct_field_layouts = collect_struct_field_layouts(&module.structs);
        crate::valkyrie::mir::merge_singleton_field_layouts(module, &mut struct_field_layouts);
        merge_imported_struct_field_layouts(module, &mut struct_field_layouts);
        let singleton_accessors = crate::valkyrie::mir::singleton_accessor_map(module);
        let mut struct_parent_index = collect_struct_parent_index(&module.structs);
        merge_imported_struct_parent_index(module, &mut struct_parent_index);
        let mut struct_is_value_type = collect_struct_is_value_type(&module.structs);
        merge_imported_struct_is_value_type(module, &mut struct_is_value_type);
        let mut aggregate_layouts = value_semantics::compute_aggregate_layout_plan(module);
        let (sum_types, _) = crate::valkyrie::hir::lowering::compute_nominal_layouts(module);
        value_semantics::ensure_unite_layouts_for_sums(&mut aggregate_layouts, &sum_types);
        let effectful_resume_map = collect_effectful_resume_map(module);
        let structs: Vec<_> = module.structs.iter().map(lower_struct).collect();
        let imports: Vec<_> = module.imports.iter().map(|import| import.path.to_string()).collect();
        let external_calls = collect_external_call_contracts(module);
        let mut functions = Vec::new();
        for (index, function) in module.functions.iter().enumerate() {
            let trace_function = index < 3 || (490..=510).contains(&index);
            if trace_function {
                eprintln!("[seed-debug] mir-function-start index={} name={}", index + 1, function.name);
            }
            if index == 0 || (index + 1) % 100 == 0 || index + 1 == module.functions.len() {
                eprintln!("[seed-debug] mir-function-progress {}/{} name={}", index + 1, module.functions.len(), function.name);
            }
            functions.push(lower_function_semantic(
                module,
                function,
                &return_types,
                &struct_field_layouts,
                &struct_parent_index,
                &struct_is_value_type,
                &mut aggregate_layouts,
                &singleton_accessors,
                &effectful_resume_map,
                None,
            ));
            if trace_function {
                eprintln!("[seed-debug] mir-function-done index={} name={}", index + 1, function.name);
            }
        }
        functions.extend(lower_singleton_method_functions(
            module,
            &return_types,
            &struct_field_layouts,
            &struct_parent_index,
            &struct_is_value_type,
            &mut aggregate_layouts,
            &singleton_accessors,
            &effectful_resume_map,
        ));
        functions.extend(lower_impl_method_functions(
            module,
            &return_types,
            &struct_field_layouts,
            &struct_parent_index,
            &struct_is_value_type,
            &mut aggregate_layouts,
            &singleton_accessors,
            &effectful_resume_map,
        ));
        eprintln!("[seed-debug] mir-lower-done module={} functions={} structs={}", module.name, functions.len(), structs.len());
        // ADR 0011: MirFunction no longer carries per-function diagnostics.
        let result = MirModule {
            name: module.name.to_string(),
            functions,
            structs,
            imports,
            external_calls,
            aggregate_layouts,
            sum_types,
            diagnostics: Vec::new(),
        };
        result
    }

    pub fn lower_module(module: &HirModule) -> MirModule {
        let mut return_types = collect_module_return_types(module);
        return_types.extend(crate::valkyrie::mir::collect_singleton_return_types(module));
        let mut struct_field_layouts = collect_struct_field_layouts(&module.structs);
        crate::valkyrie::mir::merge_singleton_field_layouts(module, &mut struct_field_layouts);
        merge_imported_struct_field_layouts(module, &mut struct_field_layouts);
        let singleton_accessors = crate::valkyrie::mir::singleton_accessor_map(module);
        let mut struct_parent_index = collect_struct_parent_index(&module.structs);
        merge_imported_struct_parent_index(module, &mut struct_parent_index);
        let mut struct_is_value_type = collect_struct_is_value_type(&module.structs);
        merge_imported_struct_is_value_type(module, &mut struct_is_value_type);
        let mut aggregate_layouts = value_semantics::compute_aggregate_layout_plan(module);
        let (sum_types, _) = crate::valkyrie::hir::lowering::compute_nominal_layouts(module);
        value_semantics::ensure_unite_layouts_for_sums(&mut aggregate_layouts, &sum_types);
        let effectful_resume_map = collect_effectful_resume_map(module);
        let structs = module.structs.iter().map(lower_struct).collect();
        let imports = module.imports.iter().map(|import| import.path.to_string()).collect();
        let external_calls = collect_external_call_contracts(module);
        let mut functions = Vec::new();
        for function in &module.functions {
            functions.push(lower_function(
                module,
                function,
                &return_types,
                &struct_field_layouts,
                &struct_parent_index,
                &struct_is_value_type,
                &mut aggregate_layouts,
                &singleton_accessors,
                &effectful_resume_map,
                None,
            ));
        }
        functions.extend(lower_singleton_method_functions(
            module,
            &return_types,
            &struct_field_layouts,
            &struct_parent_index,
            &struct_is_value_type,
            &mut aggregate_layouts,
            &singleton_accessors,
            &effectful_resume_map,
        ));
        functions.extend(lower_impl_method_functions(
            module,
            &return_types,
            &struct_field_layouts,
            &struct_parent_index,
            &struct_is_value_type,
            &mut aggregate_layouts,
            &singleton_accessors,
            &effectful_resume_map,
        ));
        // ADR 0011: MirFunction no longer carries per-function diagnostics.
        MirModule {
            name: module.name.to_string(),
            functions,
            structs,
            imports,
            external_calls,
            aggregate_layouts,
            sum_types,
            diagnostics: Vec::new(),
        }
    }
}

fn collect_external_call_contracts(module: &HirModule) -> Vec<MirExternalCallContract> {
    module
        .imported_semantic_exports
        .iter()
        .flat_map(|export| {
            export.functions.iter().map(move |function| {
                let symbol = if function.declaring_namespace.parts().is_empty() {
                    let mut parts = export.module.parts().to_vec();
                    parts.push(function.name.clone());
                    NamePath::new(parts)
                }
                else {
                    let mut parts = function.declaring_namespace.parts().to_vec();
                    parts.push(function.name.clone());
                    NamePath::new(parts)
                };
                MirExternalCallContract { symbol }
            })
        })
        .collect()
}

fn lower_singleton_method_functions(
    module: &HirModule,
    return_types: &BTreeMap<String, ValkyrieType>,
    struct_field_layouts: &BTreeMap<String, Vec<(String, ValkyrieType)>>,
    struct_parent_index: &BTreeMap<String, Vec<String>>,
    struct_is_value_type: &BTreeMap<String, bool>,
    aggregate_layouts: &mut AggregateLayoutPlan,
    singleton_accessors: &BTreeMap<String, String>,
    effectful_resume_map: &BTreeMap<String, ValkyrieType>,
) -> Vec<MirFunction> {
    let mut functions = Vec::new();
    for singleton in &module.singletons {
        for method in singleton.methods.iter().filter(|method| !method.is_abstract) {
            let mut mir_function = lower_function_semantic(
                module,
                method,
                return_types,
                struct_field_layouts,
                struct_parent_index,
                struct_is_value_type,
                aggregate_layouts,
                singleton_accessors,
                effectful_resume_map,
                Some(ValkyrieType::Named(singleton.name.clone())),
            );
            mir_function.symbol = format!("{}.{}", singleton.name, method.name);
            functions.push(mir_function);
        }
        if let Some(constructor) = &singleton.constructor {
            let mut mir_function = lower_function_semantic(
                module,
                constructor,
                return_types,
                struct_field_layouts,
                struct_parent_index,
                struct_is_value_type,
                aggregate_layouts,
                singleton_accessors,
                effectful_resume_map,
                Some(ValkyrieType::Named(singleton.name.clone())),
            );
            mir_function.symbol = format!("{}.{}", singleton.name, constructor.name);
            functions.push(mir_function);
        }
        if let Some(finalizer) = &singleton.finalizer {
            let mut mir_function = lower_function_semantic(
                module,
                finalizer,
                return_types,
                struct_field_layouts,
                struct_parent_index,
                struct_is_value_type,
                aggregate_layouts,
                singleton_accessors,
                effectful_resume_map,
                Some(ValkyrieType::Named(singleton.name.clone())),
            );
            mir_function.symbol = format!("{}.{}", singleton.name, finalizer.name);
            functions.push(mir_function);
        }
    }
    functions
}

fn lower_impl_method_functions(
    module: &HirModule,
    return_types: &BTreeMap<String, ValkyrieType>,
    struct_field_layouts: &BTreeMap<String, Vec<(String, ValkyrieType)>>,
    struct_parent_index: &BTreeMap<String, Vec<String>>,
    struct_is_value_type: &BTreeMap<String, bool>,
    aggregate_layouts: &mut AggregateLayoutPlan,
    singleton_accessors: &BTreeMap<String, String>,
    effectful_resume_map: &BTreeMap<String, ValkyrieType>,
) -> Vec<MirFunction> {
    let mut functions = Vec::new();
    for impl_block in &module.impls {
        let Some(type_name) = nominal_type_name(&impl_block.target)
        else {
            continue;
        };
        for method in impl_block.methods.iter().filter(|method| !method.is_abstract) {
            let mut mir_function = lower_function_semantic(
                module,
                method,
                return_types,
                struct_field_layouts,
                struct_parent_index,
                struct_is_value_type,
                aggregate_layouts,
                singleton_accessors,
                effectful_resume_map,
                Some(impl_block.target.clone()),
            );
            mir_function.symbol = format!("{type_name}.{}", method.name);
            functions.push(mir_function);
        }
    }
    for item in &module.structs {
        for method in item.methods.iter().filter(|method| !method.is_abstract) {
            let mut mir_function = lower_function_semantic(
                module,
                method,
                return_types,
                struct_field_layouts,
                struct_parent_index,
                struct_is_value_type,
                aggregate_layouts,
                singleton_accessors,
                effectful_resume_map,
                Some(ValkyrieType::Named(item.name.clone())),
            );
            mir_function.symbol = format!("{}.{}", item.name, method.name);
            functions.push(mir_function);
        }
    }
    for trait_def in &module.traits {
        for method in trait_def.default_methods.iter().filter(|method| !method.is_abstract) {
            let mut mir_function = lower_function_semantic(
                module,
                method,
                return_types,
                struct_field_layouts,
                struct_parent_index,
                struct_is_value_type,
                aggregate_layouts,
                singleton_accessors,
                effectful_resume_map,
                Some(ValkyrieType::Named(trait_def.name.clone())),
            );
            mir_function.symbol = format!("{}.{}", trait_def.name, method.name);
            functions.push(mir_function);
        }
    }
    functions
}

/// 将 `HirStruct` 降级为 `MirStruct`，保留字段类型供后端生成 `TypeDef` / `Field`。
fn lower_struct(hir_struct: &crate::types::hir::HirStruct) -> MirStruct {
    let fields = hir_struct.fields.iter().map(|field| MirField { name: field.name.to_string(), ty: field.ty.clone() }).collect();
    let namespace = hir_struct.namespace.iter().map(|part| part.as_str().to_string()).collect::<Vec<_>>().join(".");
    MirStruct { name: hir_struct.name.to_string(), namespace, fields, is_value_type: hir_struct.is_value_type }
}

fn collect_module_return_types(module: &HirModule) -> BTreeMap<String, ValkyrieType> {
    let mut map = module
        .functions
        .iter()
        .map(|function| (stable_hir_function_symbol(&module.name, function), function.return_type.clone()))
        .collect::<BTreeMap<_, _>>();
    for impl_block in &module.impls {
        let Some(type_name) = nominal_type_name(&impl_block.target)
        else {
            continue;
        };
        for method in &impl_block.methods {
            let return_ty = resolve_self_type_with_owner(&method.return_type, Some(&impl_block.target));
            map.insert(format!("{type_name}.{}", method.name), return_ty.clone());
            map.insert(method.name.to_string(), return_ty);
        }
    }
    for item in &module.structs {
        let owner = ValkyrieType::Named(item.name.clone());
        for method in &item.methods {
            let return_ty = resolve_self_type_with_owner(&method.return_type, Some(&owner));
            map.insert(format!("{}.{}", item.name, method.name), return_ty.clone());
            map.insert(method.name.to_string(), return_ty);
        }
    }
    map
}

/// Replace `Self` / `Named("Self")` with the imply/struct owner type.
fn resolve_self_type_with_owner(ty: &ValkyrieType, owner: Option<&ValkyrieType>) -> ValkyrieType {
    match (ty, owner) {
        (ValkyrieType::SelfType, Some(owner)) => owner.clone(),
        (ValkyrieType::Named(name), Some(owner)) if name.as_str() == "Self" => owner.clone(),
        (ValkyrieType::Array(inner), _) => ValkyrieType::Array(Box::new(resolve_self_type_with_owner(inner, owner))),
        (ValkyrieType::Apply(base, args), _) => ValkyrieType::Apply(
            Box::new(resolve_self_type_with_owner(base, owner)),
            args.iter().map(|arg| resolve_self_type_with_owner(arg, owner)).collect(),
        ),
        (ValkyrieType::Tuple(items), _) => ValkyrieType::Tuple(items.iter().map(|item| resolve_self_type_with_owner(item, owner)).collect()),
        (ValkyrieType::Union(items), _) => ValkyrieType::Union(items.iter().map(|item| resolve_self_type_with_owner(item, owner)).collect()),
        (ValkyrieType::Function(func), _) => ValkyrieType::Function(Box::new(crate::types::hir::FunctionType {
            params: func.params.iter().map(|param| resolve_self_type_with_owner(param, owner)).collect(),
            return_type: resolve_self_type_with_owner(&func.return_type, owner),
        })),
        _ => ty.clone(),
    }
}

fn collect_struct_field_layouts(structs: &[crate::types::hir::HirStruct]) -> BTreeMap<String, Vec<(String, ValkyrieType)>> {
    structs
        .iter()
        .map(|hir_struct| {
            (hir_struct.name.to_string(), hir_struct.fields.iter().map(|field| (field.name.to_string(), field.ty.clone())).collect())
        })
        .collect()
}

fn collect_struct_parent_index(structs: &[crate::types::hir::HirStruct]) -> BTreeMap<String, Vec<String>> {
    structs
        .iter()
        .map(|hir_struct| {
            (
                hir_struct.name.to_string(),
                hir_struct.parents.iter().filter_map(|parent| parent.name.parts().last().map(|name| name.to_string())).collect(),
            )
        })
        .collect()
}

fn collect_struct_is_value_type(structs: &[crate::types::hir::HirStruct]) -> BTreeMap<String, bool> {
    structs.iter().map(|hir_struct| (hir_struct.name.to_string(), hir_struct.is_value_type)).collect()
}

fn merge_imported_struct_field_layouts(module: &HirModule, layouts: &mut BTreeMap<String, Vec<(String, ValkyrieType)>>) {
    for export in &module.imported_semantic_exports {
        for (name, fields) in collect_struct_field_layouts(&export.structs) {
            layouts.entry(name).or_insert(fields);
        }
    }
}

fn merge_imported_struct_parent_index(module: &HirModule, parents: &mut BTreeMap<String, Vec<String>>) {
    for export in &module.imported_semantic_exports {
        for (name, parent_list) in collect_struct_parent_index(&export.structs) {
            parents.entry(name).or_insert(parent_list);
        }
    }
}

fn merge_imported_struct_is_value_type(module: &HirModule, is_value_type: &mut BTreeMap<String, bool>) {
    for export in &module.imported_semantic_exports {
        for (name, value) in collect_struct_is_value_type(&export.structs) {
            is_value_type.entry(name).or_insert(value);
        }
    }
}

fn lower_function(
    module: &HirModule,
    function: &HirFunction,
    return_types: &BTreeMap<String, ValkyrieType>,
    struct_field_layouts: &BTreeMap<String, Vec<(String, ValkyrieType)>>,
    struct_parent_index: &BTreeMap<String, Vec<String>>,
    struct_is_value_type: &BTreeMap<String, bool>,
    aggregate_layouts: &mut AggregateLayoutPlan,
    singleton_accessors: &BTreeMap<String, String>,
    effectful_resume_map: &BTreeMap<String, ValkyrieType>,
    impl_owner_type: Option<ValkyrieType>,
) -> MirFunction {
    let mut function = lower_function_semantic(
        module,
        function,
        return_types,
        struct_field_layouts,
        struct_parent_index,
        struct_is_value_type,
        aggregate_layouts,
        singleton_accessors,
        effectful_resume_map,
        impl_owner_type,
    );
    function
}

fn lower_function_semantic(
    module: &HirModule,
    function: &HirFunction,
    return_types: &BTreeMap<String, ValkyrieType>,
    struct_field_layouts: &BTreeMap<String, Vec<(String, ValkyrieType)>>,
    struct_parent_index: &BTreeMap<String, Vec<String>>,
    struct_is_value_type: &BTreeMap<String, bool>,
    aggregate_layouts: &mut AggregateLayoutPlan,
    singleton_accessors: &BTreeMap<String, String>,
    effectful_resume_map: &BTreeMap<String, ValkyrieType>,
    impl_owner_type: Option<ValkyrieType>,
) -> MirFunction {
    let effectful_inline_targets = effect_lowering::collect_effectful_inline_targets(module);
    let (sum_types, _) = crate::valkyrie::hir::lowering::compute_nominal_layouts(module);
    value_semantics::ensure_unite_layouts_for_sums(aggregate_layouts, &sum_types);
    let mut builder = MirBuilder::new(
        return_types.clone(),
        struct_field_layouts.clone(),
        struct_parent_index.clone(),
        struct_is_value_type.clone(),
        aggregate_layouts.clone(),
        singleton_accessors.clone(),
        effectful_resume_map.clone(),
        effectful_inline_targets,
        impl_owner_type.clone(),
    );
    builder.sum_types = sum_types;
    // Imply/struct methods declare `: Self`; keep MIR return/params on the owner
    // so SMIR007 compares against `isize`/`i32` (etc.), not unsubstituted `Self`.
    let resolved_return_type = resolve_self_type_with_owner(&function.return_type, impl_owner_type.as_ref());
    builder.current_return_type = resolved_return_type.clone();

    // 为函数参数分配 SSA 值，并登记到绑定表与入口块 `parameters`。
    // `self` 参数在 HIR 里常为 `AutoType`/`SelfType`（无显式标注），这里用 impl owner
    // 类型名替换，使后续 `field_type_for_object_operand` 能查到泛型结构体字段类型。
    let mut param_values = Vec::new();
    for (index, param) in function.params.iter().enumerate() {
        let value = builder.next_value(MirValueOrigin::Parameter { index, name: param.name.name.to_string() });
        builder.bindings.insert(param.name.name.to_string(), MirOperand::Value(value));
        let resolved_ty = match (&param.ty, impl_owner_type.as_ref()) {
            (ValkyrieType::AutoType, Some(owner)) => owner.clone(),
            (ty, owner) => resolve_self_type_with_owner(ty, owner),
        };
        builder.value_types.insert(value, resolved_ty);
        param_values.push(value);
    }

    // 入口块参数列表与函数形参一一对应。
    // 对 CLR 等后端，可据此走 `collect_parameter_slots` 并生成 `ldarg`。
    builder.blocks[0].parameters = param_values.clone();

    // 按顺序 lowering 函数体语句。
    if builder.terminator.is_none() {
        for statement in &function.body.statements {
            builder.lower_statement(statement);
            // 若语句已产生 return 等终结符，则停止继续 lowering，
            // 避免不可达代码进入后续块。
            if builder.terminator.is_some() {
                break;
            }
        }
    }

    // 处理尾表达式。
    if builder.terminator.is_none() {
        if let Some(expr) = &function.body.expr {
            let return_ty = builder.current_return_type.clone();
            // Unit-returning mutators (e.g. ArrayList.push) often end with a call that
            // yields a non-Unit value (`push(self._items, value) -> [T]`). Evaluate the
            // trailing expr for effects, then return Unit — do not promote the call
            // result into Return (SMIR007: actual=Array(T) expected=Unit).
            if matches!(return_ty, ValkyrieType::Unit) {
                let _ = builder.lower_expr_to_operand(expr);
                if builder.terminator.is_none() {
                    builder.terminate(MirTerminator::Return { value: None });
                }
            }
            else {
                let operand = builder.lower_expr_to_operand_with_hint(expr, Some(&return_ty));
                if builder.terminator.is_none() {
                    builder.terminate(MirTerminator::Return { value: Some(operand) });
                }
            }
        }
        else {
            builder.terminate(MirTerminator::Return { value: None });
        }
    }

    // 刷新当前块指令到 `blocks`。
    let current_label = builder.current_label.clone();
    builder.flush_block(&current_label);
    value_semantics::merge_aggregate_layout_plan(aggregate_layouts, &builder.aggregate_layouts);

    let param_types = function
        .params
        .iter()
        .map(|p| match (&p.ty, impl_owner_type.as_ref()) {
            (ValkyrieType::AutoType, Some(owner)) => owner.clone(),
            (ty, owner) => resolve_self_type_with_owner(ty, owner),
        })
        .collect();
    let mut mir_function = MirFunction {
        symbol: stable_hir_function_symbol(&module.name, function),
        return_type: resolved_return_type,
        param_types,
        value_types: builder.value_types,
        entry: builder.entry,
        values: builder.values,
        blocks: builder.blocks,
    };
    // DELETED (ADR 0011): suspend_analysis / frame_planning attached God metadata onto MirFunction.
    mir_function
}

/// `MIR` 函数体 lowering 构建器，维护当前块、绑定与效应上下文。
struct MirBuilder {
    entry: MirBlockRef,
    current_block: MirBlockRef,
    current_label: String,
    values: Vec<MirValue>,
    instructions: Vec<MirInstruction>,
    blocks: Vec<MirBlock>,
    suspend_points: Vec<MirSuspendPoint>,
    continuations: Vec<MirContinuation>,
    case_chains: Vec<MirCaseChain>,
    bindings: BTreeMap<String, MirOperand>,
    value_types: BTreeMap<MirValueRef, ValkyrieType>,
    return_types: BTreeMap<String, ValkyrieType>,
    struct_field_layouts: BTreeMap<String, Vec<(String, ValkyrieType)>>,
    struct_parent_index: BTreeMap<String, Vec<String>>,
    struct_is_value_type: BTreeMap<String, bool>,
    aggregate_layouts: AggregateLayoutPlan,
    /// Unite/enum layouts for nullary variant `tag` compares (`case LeftBrace:`).
    sum_types: Vec<SumTypeLayout>,
    singleton_accessors: BTreeMap<String, String>,
    static_bindings: BTreeMap<String, HirExpr>,
    terminator: Option<MirTerminator>,
    value_seed: u32,
    /// Dense InstructionId allocator (ADR 0012).
    instruction_seed: u32,
    /// Dense ProvenanceId allocator (ADR 0012).
    provenance_seed: u32,
    state_seed: u32,
    /// Unified loop / try / fallthrough scope tracking for lowering.
    control_flow: MirBuilderControlFlow,
    /// 嵌套 handler arm 深度：位于 catch/handler 体内时递增。
    /// Arm 内 `raise` 需要绑定到当前 handler；深度为 0 表示不在任何 handler 内。
    suspended_handler_depth: usize,
    /// 当前函数返回类型；用于 `?` 传播等需要对照返回类型的路径。
    current_return_type: ValkyrieType,
    /// `MIR` lowering 过程中收集的编译期诊断。
    diagnostics: Vec<MirDiagnostic>,
    /// 模块级 `Effectful::Resume` 关联类型映射（载体类型名 → Resume 类型）。
    ///
    /// 由 [`collect_effectful_resume_map`] 在 lowering 入口预先构造，
    /// 供 `raise` 路径查询用户 effect 载体的 resume 类型，以正确推断 resume 参数类型。
    effectful_resume_map: BTreeMap<String, ValkyrieType>,
    /// 可在 `catch` 调用点内联的效应函数（函数名 → 函数定义）。
    ///
    /// 由 [`effect_lowering::collect_effectful_inline_targets`] 在 lowering 入口预先构造。
    /// 仅包含含 `raise`、无 `return`、非递归的模块级函数。`lower_catch_expr` 在调用点
    /// 检查被调用函数是否在此映射中，若是则内联其函数体以实现跨函数 `raise` 传播。
    effectful_inline_targets: BTreeMap<String, HirFunction>,
    /// 当前 impl 方法的 owner 类型名（如 `FilterIterator`）。
    ///
    /// 用于把 `self` 参数的 `AutoType`/`SelfType` 解析为具体命名类型，
    /// 使 `field_type_for_object_operand` 能查到泛型结构体字段类型。
    /// 自由函数为 `None`。
    impl_owner_type: Option<ValkyrieType>,
}

impl MirBuilder {
    fn new(
        return_types: BTreeMap<String, ValkyrieType>,
        struct_field_layouts: BTreeMap<String, Vec<(String, ValkyrieType)>>,
        struct_parent_index: BTreeMap<String, Vec<String>>,
        struct_is_value_type: BTreeMap<String, bool>,
        aggregate_layouts: AggregateLayoutPlan,
        singleton_accessors: BTreeMap<String, String>,
        effectful_resume_map: BTreeMap<String, ValkyrieType>,
        effectful_inline_targets: BTreeMap<String, HirFunction>,
        impl_owner_type: Option<ValkyrieType>,
    ) -> Self {
        let entry = MirBlockRef(0);
        Self {
            entry,
            current_block: entry,
            current_label: "entry".to_string(),
            values: Vec::new(),
            instructions: Vec::new(),
            blocks: vec![MirBlock {
                id: entry,
                label: "entry".to_string(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: MirTerminator::Unreachable,
            }],
            suspend_points: Vec::new(),
            continuations: Vec::new(),
            case_chains: Vec::new(),
            bindings: BTreeMap::new(),
            value_types: BTreeMap::new(),
            return_types,
            struct_field_layouts,
            struct_parent_index,
            struct_is_value_type,
            aggregate_layouts,
            sum_types: Vec::new(),
            singleton_accessors,
            static_bindings: BTreeMap::new(),
            terminator: None,
            value_seed: 0,
            instruction_seed: 0,
            provenance_seed: 0,
            state_seed: 0,
            suspended_handler_depth: 0,
            control_flow: MirBuilderControlFlow::default(),
            current_return_type: ValkyrieType::Unit,
            diagnostics: Vec::new(),
            effectful_resume_map,
            effectful_inline_targets,
            impl_owner_type,
        }
    }

    fn next_value(&mut self, origin: MirValueOrigin) -> MirValueRef {
        let id = MirValueRef(self.value_seed);
        self.value_seed += 1;
        self.values.push(MirValue { id, origin });
        id
    }

    /// Push an instruction with fresh dense [`InstructionId`] + [`ProvenanceId`] (ADR 0012).
    pub(super) fn push_instruction(&mut self, operation: MirOperation, results: Vec<MirValueRef>) {
        let id = nyar_types::InstructionId::from_index(self.instruction_seed).expect("InstructionId");
        self.instruction_seed = self.instruction_seed.saturating_add(1);
        let provenance = nyar_types::ProvenanceId::from_index(self.provenance_seed).expect("ProvenanceId");
        self.provenance_seed = self.provenance_seed.saturating_add(1);
        self.instructions.push(MirInstruction { id, results, kind: operation, provenance });
    }

    fn next_state_id(&mut self) -> u32 {
        let id = self.state_seed;
        self.state_seed += 1;
        id
    }

    fn terminate(&mut self, terminator: MirTerminator) {
        self.terminator = Some(terminator);
    }

    fn new_block(&mut self, label: &str) -> MirBlockRef {
        let id = MirBlockRef(self.blocks.len() as u32);
        self.blocks.push(MirBlock {
            id,
            label: label.to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: MirTerminator::Unreachable,
        });
        self.current_block = id;
        self.current_label = label.to_string();
        id
    }

    fn flush_block(&mut self, label: &str) {
        // 将缓冲中的指令写入 current_block；块 ID 保持不变，仅更新标签与内容。
        let block = &mut self.blocks[self.current_block.0 as usize];
        block.label = label.to_string();
        block.instructions = self.instructions.clone();
        block.terminator = self.terminator.clone().unwrap_or(MirTerminator::Unreachable);
        // 清空指令缓冲与终结符，准备写入下一个块。
        self.instructions.clear();
        self.terminator = None;
    }

    fn ensure_loop_exit_parameter(&mut self, loop_index: usize, ty: Option<ValkyrieType>) -> MirValueRef {
        if let Some(value) = self.control_flow.loop_at(loop_index).exit_value {
            return value;
        }

        let exit_block = self.control_flow.loop_at(loop_index).exit;
        if let Some(value) = self.blocks[exit_block.0 as usize].parameters.first().copied() {
            if let Some(ty) = ty {
                self.value_types.entry(value).or_insert(ty);
            }
            self.control_flow.loop_at_mut(loop_index).exit_value = Some(value);
            return value;
        }

        let value = self.next_value(MirValueOrigin::Temporary);
        if let Some(ty) = ty {
            self.value_types.insert(value, ty);
        }
        self.blocks[exit_block.0 as usize].parameters.push(value);
        self.control_flow.loop_at_mut(loop_index).exit_value = Some(value);
        value
    }

    fn ensure_block_parameter(&mut self, block: MirBlockRef, name: &str, ty: Option<ValkyrieType>) -> MirValueRef {
        if let Some(value) = self.blocks[block.0 as usize].parameters.first().copied() {
            if let Some(ty) = ty {
                self.value_types.entry(value).or_insert(ty);
            }
            return value;
        }

        let value = self.next_value(MirValueOrigin::BlockParameter { block, name: name.to_string() });
        if let Some(ty) = ty {
            self.value_types.insert(value, ty);
        }
        self.blocks[block.0 as usize].parameters.push(value);
        value
    }

    fn ensure_handler_exit_parameter(&mut self, handler_index: usize, ty: Option<ValkyrieType>) -> MirValueRef {
        if let Some(value) = self.control_flow.handler_at(handler_index).exit_value {
            return value;
        }

        let exit_block = self.control_flow.handler_at(handler_index).exit;
        if let Some(value) = self.blocks[exit_block.0 as usize].parameters.first().copied() {
            if let Some(ty) = ty {
                self.value_types.entry(value).or_insert(ty);
            }
            self.control_flow.handler_at_mut(handler_index).exit_value = Some(value);
            return value;
        }

        let value = self.next_value(MirValueOrigin::Temporary);
        if let Some(ty) = ty {
            self.value_types.insert(value, ty);
        }
        self.blocks[exit_block.0 as usize].parameters.push(value);
        self.control_flow.handler_at_mut(handler_index).exit_value = Some(value);
        value
    }

    fn resolve_loop_index(&self, label: Option<&Identifier>) -> Option<usize> {
        self.control_flow.resolve_loop_index(label)
    }

    fn bind_catch_arm_pattern(&mut self, pattern: &HirPattern, payload: MirOperand, extractor_payload: Option<MirOperand>) {
        match pattern {
            HirPattern::Else | HirPattern::Wildcard => {}
            _ => self.bind_pattern_from_operand_with_payload(pattern, payload, None, extractor_payload),
        }
    }

    fn lower_statement(&mut self, statement: &HirStatement) {
        match &statement.kind {
            HirStatementKind::Let { pattern, initializer, ty, .. } => {
                self.record_static_binding(pattern, initializer.as_deref());
                if let Some(expr) = initializer.as_deref() {
                    self.bind_pattern_from_expr(pattern, expr, ty.clone());
                }
                else {
                    self.bind_pattern_from_operand(pattern, MirOperand::Constant(MirConstant::Unit), ty.clone());
                }
            }
            HirStatementKind::Expr(expression) => {
                let _ = self.lower_expr_to_operand(expression);
            }
        }
    }

    fn lower_static_call(&mut self, name: &str, arguments: Vec<MirOperand>, origin: MirValueOrigin) -> MirValueRef {
        if name == "__ref_deref" {
            if let Some(MirOperand::Value(value)) = arguments.first() {
                return *value;
            }
        }
        let value = self.next_value(origin);
        let parameter_types =
            arguments.iter().map(|argument| infer_builder_operand_type(argument, &self.value_types)).collect::<Option<Vec<_>>>();
        self.instructions.push(MirInstruction::from_operation(MirOperation::Call {
            callee: MirOperand::Symbol(NamePath::new(vec![Identifier::new(name)])),
            arguments,
        }));
        if let Some(return_type) = self.return_types.get(name).cloned() {
            self.value_types.insert(value, return_type);
        }
        value
    }
}
