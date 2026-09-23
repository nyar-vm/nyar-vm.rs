#![doc = include_str!("readme.md")]

pub mod validation;

use std::collections::BTreeMap;

use crate::{
    hir::ValkyrieCompiler,
    mir::{
        MirBlock, MirBlockRef, MirConstant, MirEffectKind, MirFunction, MirInstruction, MirLowerer, MirModule, MirOperand, MirOperation,
        MirTerminator, MirValueRef,
    },
    symbols::stable_hir_function_symbol,
    types::{
        NamePath,
        hir::{HirModule, HirPattern, ValkyrieType as HirType},
    },
    validation::ControlFlowScheduler,
};
use std_data::text::valkyrie::{AstParser, ParseError, ValkyrieRoot};

pub use validation::validate_module;

/// Legacy target-aware low-level representation module.
#[deprecated(note = "legacy `LIR` 正在移除，请改用 `compile_source_to_build_output()`、`FrontendBuildOutput` 与 `FrontendNeutralPlan`。")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LirModule {
    /// Logical module name.
    pub name: String,
    /// 结构体定义，保留字段类型供后端继续推导 `FieldGet` / `FieldSet`。
    pub structs: Vec<crate::mir::MirStruct>,
    /// Functions lowered into the legacy low-level view.
    pub functions: Vec<LirFunction>,
}

impl ValkyrieCompiler {
    /// Parses source text and lowers it into the legacy `LIR` view.
    #[deprecated(
        note = "legacy `LIR` facade 正在移除，请改用 `compile_source()` / `compile_source_to_mir()` + `FrontendBuildOutput` / `FrontendNeutralPlan`。"
    )]
    pub fn compile_source_to_lir(&self, source: &str) -> Result<LirModule, ParseError> {
        let root = AstParser::parse_root(source)?;
        self.lower_root_to_lir(&root)
    }

    /// Lowers parser output into the legacy `LIR` view through the compatibility pipeline.
    #[deprecated(note = "legacy `LIR` lowering 正在移除，请改用 `FrontendBuildOutput` / `FrontendNeutralPlan` 主链。")]
    pub fn lower_root_to_lir(&self, root: &ValkyrieRoot) -> Result<LirModule, ParseError> {
        let hir = self.lower_root(root)?;
        ControlFlowScheduler::validate_hir_module(&hir)?;
        let mir = MirLowerer::lower_module(&hir);
        ControlFlowScheduler::validate_mir_module(&mir)?;
        let lir = LirLowerer::lower_mir_module(&hir, &mir);
        validation::validate_module(&lir)?;
        Ok(lir)
    }
}

/// Low-level function body grouped by basic blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LirFunction {
    /// Symbol name of the lowered function.
    pub symbol: String,
    /// 函数参数类型列表，用于后端生成调用约定与方法签名。
    pub param_types: Vec<HirType>,
    /// 函数返回类型，用于后端判断调用是否返回 `void`。
    pub return_type: HirType,
    /// `SSA` 值的静态类型表，从 `MIR` 透传，供调度校验使用。
    pub value_types: BTreeMap<MirValueRef, HirType>,
    /// Suspend 点元数据，从 `MIR` 透传，供 lane lowering / frame 构造使用。
    pub suspend_points: Vec<LirSuspendPoint>,
    /// Frame layout 计划，从 `MIR` 透传，供 lane/runtime lowering 直接消费。
    pub frame_layouts: Vec<LirFrameLayout>,
    /// Continuation 元数据，从 `MIR` 透传，供 lane lowering 与调度校验使用。
    pub continuations: Vec<LirContinuation>,
    /// `case` / `match` 链路元数据，从 `MIR` 透传，供 arm merge 与 `fallthrough` 校验使用。
    pub case_chains: Vec<LirCaseChain>,
    /// 显式 runtime frame 载体，作为 lane/runtime 消费 `frame_layouts` 的稳定边界。
    pub runtime_frames: Vec<LirRuntimeFrame>,
    /// 显式 runtime continuation 载体，作为 handler/runtime 消费恢复协议的稳定边界。
    pub runtime_continuations: Vec<LirRuntimeContinuation>,
    /// 状态机 emit 描述，从 `MIR` 保真透传。
    pub state_machine: Option<LirStateMachineDescriptor>,
    /// Entry block of the function.
    pub entry: MirBlockRef,
    /// All basic blocks that belong to the function.
    pub blocks: Vec<LirBlock>,
}

/// Low-level basic block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LirBlock {
    /// Stable block identifier.
    pub id: MirBlockRef,
    /// Human-readable block label.
    pub label: String,
    /// Incoming block parameters.
    pub parameters: Vec<MirValueRef>,
    /// Non-terminating low-level operations.
    pub operations: Vec<LirOperation>,
    /// Explicit block terminator.
    pub terminator: LirTerminator,
}

/// 低层 continuation 元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LirContinuation {
    /// 对应的 handler dispatch block。
    pub dispatch_block: MirBlockRef,
    /// Continuation 恢复时跳回的 block。
    pub resume_target: MirBlockRef,
    /// 恢复值进入的 block parameter。
    pub resume_parameter: MirValueRef,
    /// 当前已知的恢复值类型。
    pub resume_parameter_type: Option<HirType>,
    /// Handler 正常结束时汇入的 exit block。
    pub handler_exit: MirBlockRef,
    /// 触发该 continuation 的用户 effect 载体类型（仅 `Raise` 时填入）。
    pub carrier_type: Option<HirType>,
}

/// 低层 `case / match` 链路元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LirCaseChain {
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
    pub arms: Vec<LirCaseArm>,
}

/// 单个低层 `case / match arm` 的控制流元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LirCaseArm {
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

/// 低层 suspend 点元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LirSuspendPoint {
    /// 状态机 lowering 使用的显式状态编号。
    pub state_id: u32,
    /// 触发的 effect 类型。
    pub effect: LirEffectKind,
    /// 发生挂起的 block。
    pub suspend_block: MirBlockRef,
    /// 恢复时跳回的 block。
    pub resume_target: MirBlockRef,
    /// 恢复点参数个数。
    pub resume_parameter_count: usize,
    /// 当前已知 payload 的静态类型。
    pub payload_type: Option<HirType>,
    /// 后续 frame / spill lowering 可直接使用的候选 SSA 值。
    pub spill_candidates: Vec<MirValueRef>,
    /// 若挂起点位于 handler arm 内，关联的 continuation 索引。
    pub continuation_index: Option<usize>,
    /// 触发该挂起点的用户 effect 载体类型（仅 `Raise` 时填入）。
    pub carrier_type: Option<HirType>,
}

/// 低层 frame layout 计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LirFrameLayout {
    /// 对应 suspend 点的状态编号。
    pub state_id: u32,
    /// 对应的 effect 类型。
    pub effect: LirEffectKind,
    /// 恢复时跳回的 block。
    pub resume_target: MirBlockRef,
    /// 需要保存的槽位布局。
    pub slots: Vec<LirFrameSlot>,
    /// 触发该 frame 的用户 effect 载体类型（仅 `Raise` 时填入）。
    pub carrier_type: Option<HirType>,
}

/// 低层 frame spill 槽位。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LirFrameSlot {
    /// Frame 中的稳定槽位序号。
    pub slot_index: usize,
    /// 被保存的 SSA 值。
    pub value: MirValueRef,
    /// 当前已知的槽位静态类型。
    pub value_type: Option<HirType>,
}

/// 面向 lane/runtime 的显式 frame 承载实体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LirRuntimeFrame {
    /// 稳定 carrier 名称，供后续 lane lowering 映射到具体对象 / 结构体。
    pub carrier: String,
    /// 对应 suspend 点的状态编号。
    pub state_id: u32,
    /// 恢复时跳回的目标 block。
    pub resume_target: MirBlockRef,
    /// Frame 中需要承载的槽位字段。
    pub slots: Vec<LirRuntimeSlot>,
    /// 若该 frame 关联 continuation，存储对应的 continuation 索引。
    pub continuation_index: Option<usize>,
}

/// Runtime frame 中的单个字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LirRuntimeSlot {
    /// 稳定的字段名。
    pub field_name: String,
    /// 对应 `frame_layout` 中的槽位序号。
    pub slot_index: usize,
    /// 被保存的 SSA 值。
    pub value: MirValueRef,
    /// 当前已知的槽位静态类型。
    pub value_type: Option<HirType>,
}

/// 面向 lane/runtime 的显式 continuation 承载实体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LirRuntimeContinuation {
    /// 稳定 carrier 名称，供 lane lowering 映射到具体 continuation 对象。
    pub carrier: String,
    /// 对应的 handler dispatch block。
    pub dispatch_block: MirBlockRef,
    /// Continuation 恢复时跳回的 block。
    pub resume_target: MirBlockRef,
    /// 恢复值进入的 block parameter。
    pub resume_parameter: MirValueRef,
    /// Continuation 载体中承载恢复值的稳定字段名。
    pub resume_parameter_field: String,
    /// 当前已知的恢复值类型。
    pub resume_parameter_type: Option<HirType>,
    /// Handler 正常结束时汇入的 exit block。
    pub handler_exit: MirBlockRef,
    /// 若当前 continuation 已关联到显式 frame，则记录对应的状态编号。
    pub frame_state_id: Option<u32>,
}

/// 单个 emit 状态，从 `MIR` 保真透传。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LirEmittedState {
    /// 与 `MirSuspendPoint::state_id` 对齐。
    pub state_id: u32,
    /// 触发挂起的效应种类。
    pub effect: LirEffectKind,
    /// 发生挂起的基本块。
    pub suspend_block: MirBlockRef,
    /// 恢复后进入的基本块。
    pub resume_target: MirBlockRef,
    /// 恢复点参数个数。
    pub resume_parameter_count: usize,
    /// 恢复点参数的静态类型。
    pub resume_parameter_type: Option<HirType>,
    /// 当前已知 payload 的静态类型。
    pub payload_type: Option<HirType>,
    /// 需要 spill 到 frame 的 `SSA` 值。
    pub spill_slots: Vec<MirValueRef>,
    /// 与 runtime frame carrier 对齐的稳定名称。
    pub frame_carrier: String,
    /// 关联的 continuation 索引。
    pub continuation_index: Option<usize>,
}

/// 函数级状态机 emit 描述，从 `MIR` 保真透传。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LirStateMachineDescriptor {
    /// 目标函数符号。
    pub function_symbol: String,
    /// 函数 entry block。
    pub entry_block: MirBlockRef,
    /// 按 `state_id` 排序的 emit 状态列表。
    pub states: Vec<LirEmittedState>,
    /// handler 入口块列表。
    pub handler_dispatch_blocks: Vec<MirBlockRef>,
}

/// Single low-level operation with optional result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LirOperation {
    /// Result value produced by this operation, if any.
    pub output: Option<MirValueRef>,
    /// Concrete low-level operation kind.
    pub kind: LirOperationKind,
}

/// Low-level operand after target-lane shaping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LirOperand {
    /// SSA value produced inside the current function.
    Value(MirValueRef),
    /// Immediate constant.
    Constant(MirConstant),
    /// Symbolic global or referenced item.
    Symbol(NamePath),
}

/// LIR must not preserve physical dispatch; RepresentationPlan owns call layout.

/// Effect terminator category preserved from `MIR`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LirEffectKind {
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

/// Concrete low-level operation family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LirOperationKind {
    /// Materializes an immediate constant.
    LoadConstant {
        /// Constant payload.
        constant: MirConstant,
        /// Optional expected type carried from earlier literal lowering.
        ty: Option<HirType>,
    },
    /// Materializes a named symbol.
    LoadSymbol {
        /// Resolved symbol path.
        path: NamePath,
    },
    /// Moves or copies an operand.
    Move {
        /// Source operand.
        source: LirOperand,
    },
    /// 存储到命名变量槽位（可变变量）。
    ///
    /// `name` 是变量名，`value` 是要存储的值。
    /// 同名 `StoreVar` 复用同一局部槽位，确保循环 header 能读到最新值。
    StoreVar {
        /// 变量名。
        name: String,
        /// 要存储的值。
        value: LirOperand,
        /// 变量声明类型注解。
        ty: Option<HirType>,
    },
    /// Lane-aware call operation.
    Call {
        /// Callee operand.
        callee: LirOperand,
        /// Positional arguments.
        arguments: Vec<LirOperand>,
    },
    /// Array allocation.
    ArrayNew {
        /// Array element type.
        element_type: HirType,
        /// Array length.
        length: LirOperand,
    },
    /// 数组字面量构造。
    ///
    /// 这是数组构造自身的内建语义，不借用通用 `[]=` 语法糖。
    /// DELETED God name ArrayLiteral; transitional LIR-only until LIR dies.
    ArrayFromElements {
        /// 数组元素类型。
        element_type: HirType,
        /// 元素列表。
        items: Vec<LirOperand>,
    },
    /// Struct construction.
    StructNew {
        /// Struct type name.
        type_name: String,
        /// Value vs reference storage (must not map value aggregates to GC classes).
        storage: crate::valkyrie::mir::MirStorageKind,
        /// Field initializers.
        fields: Vec<(String, LirOperand)>,
    },
    /// Struct field load.
    FieldGet {
        /// Object operand.
        object: LirOperand,
        /// Field name.
        field: String,
    },
    /// Struct field store.
    FieldSet {
        /// Object operand.
        object: LirOperand,
        /// Field name.
        field: String,
        /// Stored value.
        value: LirOperand,
    },
    /// 显式保留模式判定，供后续 lane lowering 与 runtime 桥接使用。
    PatternMatch {
        /// 被判定的输入值。
        value: LirOperand,
        /// 需要匹配的源级模式。
        pattern: HirPattern,
    },
}

/// Explicit low-level block terminator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LirTerminator {
    /// Returns from the current function.
    Return {
        /// Optional return value.
        value: Option<LirOperand>,
    },
    /// Unconditional jump with block arguments.
    Jump {
        /// Destination block.
        target: MirBlockRef,
        /// Outgoing block arguments.
        arguments: Vec<LirOperand>,
    },
    /// Conditional branch.
    Branch {
        /// Branch condition.
        condition: LirOperand,
        /// True edge destination.
        then_target: MirBlockRef,
        /// False edge destination.
        else_target: MirBlockRef,
    },
    /// Explicit effect suspension / delegation edge.
    PerformEffect {
        /// Effect category selected upstream.
        effect: LirEffectKind,
        /// Optional effect payload.
        payload: Option<LirOperand>,
        /// Resume target after the effect is handled.
        resume_target: MirBlockRef,
    },
    /// Terminates unreachable code.
    Unreachable,
    /// State-machine dispatch switch.
    StateDispatch { state: MirValueRef, cases: Vec<(u32, MirBlockRef)>, default_target: MirBlockRef },
    /// Suspend and hand control to runtime.
    YieldToRuntime { effect: LirEffectKind, payload: Option<LirOperand>, resume_state: u32 },
}

/// Lowers `HIR` into the legacy low-level `LIR` view.
#[deprecated(note = "legacy `LIR` lowerer 正在移除，请改用 `FrontendNeutralPlan` 与后续 `ArtifactPartitionPlan` 主链。")]
pub struct LirLowerer;

impl LirLowerer {
    /// Lowers a `MIR` module into the legacy `LIR` view.
    pub fn lower_mir_module(hir: &HirModule, mir: &MirModule) -> LirModule {
        let return_types = collect_return_types(hir);
        lower_mir_module_internal(mir, &return_types)
    }
}

fn collect_return_types(module: &HirModule) -> BTreeMap<String, HirType> {
    module.functions.iter().map(|function| (stable_hir_function_symbol(&module.name, function), function.return_type.clone())).collect()
}

fn erase_backend_opaque_type(ty: &mut HirType) {
    match ty {
        HirType::Named(name) if name.as_str() == "ExitCode" => {
            *ty = HirType::Integer32 { signed: true };
        }
        HirType::Apply(base, arguments) => {
            erase_backend_opaque_type(base);
            for argument in arguments {
                erase_backend_opaque_type(argument);
            }
        }
        HirType::Function(function) => {
            for parameter in &mut function.params {
                erase_backend_opaque_type(parameter);
            }
            erase_backend_opaque_type(&mut function.return_type);
        }
        HirType::Tuple(items) => {
            for item in items {
                erase_backend_opaque_type(item);
            }
        }
        HirType::Row(row) => {
            for method in &mut row.methods {
                for parameter in &mut method.params {
                    erase_backend_opaque_type(parameter);
                }
                erase_backend_opaque_type(&mut method.return_type);
            }
        }
        HirType::Array(inner) => {
            erase_backend_opaque_type(inner);
        }
        HirType::TypeLambda(type_lambda) => {
            erase_backend_opaque_type(&mut type_lambda.body);
        }
        HirType::TraitObject(trait_object) => {
            for argument in &mut trait_object.type_arguments {
                erase_backend_opaque_type(argument);
            }
        }
        HirType::Associated(associated) => {
            erase_backend_opaque_type(&mut associated.base);
            for argument in &mut associated.type_arguments {
                erase_backend_opaque_type(argument);
            }
        }
        _ => {}
    }
}

fn lower_mir_module_internal(module: &MirModule, return_types: &BTreeMap<String, HirType>) -> LirModule {
    LirModule {
        name: module.name.clone(),
        structs: module.structs.clone(),
        functions: module.functions.iter().map(|function| lower_mir_function(function, return_types)).collect(),
    }
}

fn lower_mir_function(function: &MirFunction, return_types: &BTreeMap<String, HirType>) -> LirFunction {
    let mut return_type = return_types.get(function.symbol.as_str()).cloned().unwrap_or(HirType::Unit);
    erase_backend_opaque_type(&mut return_type);
    // ADR 0011: Semantic MirFunction no longer carries suspend/frame/case God metadata.
    let suspend_points: Vec<LirSuspendPoint> = Vec::new();
    let frame_layouts: Vec<LirFrameLayout> = Vec::new();
    let continuations: Vec<LirContinuation> = Vec::new();
    let case_chains: Vec<LirCaseChain> = Vec::new();
    let runtime_frames = lower_runtime_frames(function, &frame_layouts, &suspend_points);
    let runtime_continuations = lower_runtime_continuations(function, &continuations, &suspend_points);
    let state_machine = None;
    let mut param_types = function.param_types.clone();
    for parameter in &mut param_types {
        erase_backend_opaque_type(parameter);
    }
    let mut value_types = function.value_types.clone();
    for value_type in value_types.values_mut() {
        erase_backend_opaque_type(value_type);
    }
    LirFunction {
        symbol: function.symbol.clone(),
        param_types,
        return_type,
        value_types,
        suspend_points,
        frame_layouts,
        continuations,
        case_chains,
        runtime_frames,
        runtime_continuations,
        state_machine,
        entry: function.entry,
        blocks: function.blocks.iter().map(lower_mir_block).collect(),
    }
}

fn lower_runtime_frames(function: &MirFunction, frame_layouts: &[LirFrameLayout], suspend_points: &[LirSuspendPoint]) -> Vec<LirRuntimeFrame> {
    let carrier_table = nyar_types::CarrierTable::new(function.symbol.as_str());
    frame_layouts
        .iter()
        .map(|layout| {
            let continuation_index = suspend_points.iter().find(|sp| sp.state_id == layout.state_id).and_then(|sp| sp.continuation_index);
            LirRuntimeFrame {
                carrier: carrier_table.frame(layout.state_id),
                state_id: layout.state_id,
                resume_target: layout.resume_target,
                slots: layout
                    .slots
                    .iter()
                    .map(|slot| LirRuntimeSlot {
                        field_name: nyar_types::CarrierTable::frame_slot_field(slot.slot_index),
                        slot_index: slot.slot_index,
                        value: slot.value,
                        value_type: slot.value_type.clone(),
                    })
                    .collect(),
                continuation_index,
            }
        })
        .collect()
}

fn lower_runtime_continuations(
    function: &MirFunction,
    continuations: &[LirContinuation],
    suspend_points: &[LirSuspendPoint],
) -> Vec<LirRuntimeContinuation> {
    let carrier_table = nyar_types::CarrierTable::new(function.symbol.as_str());
    continuations
        .iter()
        .enumerate()
        .map(|(index, continuation)| {
            let frame_state_id = suspend_points
                .iter()
                .find(|suspend_point| suspend_point.continuation_index == Some(index))
                .map(|suspend_point| suspend_point.state_id);
            LirRuntimeContinuation {
                carrier: carrier_table.continuation(index),
                dispatch_block: continuation.dispatch_block,
                resume_target: continuation.resume_target,
                resume_parameter: continuation.resume_parameter,
                resume_parameter_field: nyar_types::CarrierTable::continuation_resume_field().to_string(),
                resume_parameter_type: continuation.resume_parameter_type.clone(),
                handler_exit: continuation.handler_exit,
                frame_state_id,
            }
        })
        .collect()
}

fn lower_mir_block(block: &MirBlock) -> LirBlock {
    let operations = block.instructions.iter().map(lower_mir_instruction).collect();

    let terminator = match &block.terminator {
        MirTerminator::Return { value } => LirTerminator::Return { value: value.clone().map(lower_mir_operand) },
        MirTerminator::Jump { target, arguments } => {
            LirTerminator::Jump { target: *target, arguments: arguments.iter().cloned().map(lower_mir_operand).collect() }
        }
        MirTerminator::Branch { condition, then_target, else_target } => {
            LirTerminator::Branch { condition: lower_mir_operand(condition.clone()), then_target: *then_target, else_target: *else_target }
        }
        MirTerminator::PerformEffect { effect, payload, resume_target } => LirTerminator::PerformEffect {
            effect: lower_effect_kind(*effect),
            payload: payload.clone().map(lower_mir_operand),
            resume_target: *resume_target,
        },
        MirTerminator::StateDispatch { state, cases, default_target } => {
            LirTerminator::StateDispatch { state: *state, cases: cases.clone(), default_target: *default_target }
        }
        MirTerminator::YieldToRuntime { effect, payload, resume_state } => LirTerminator::YieldToRuntime {
            effect: lower_effect_kind(*effect),
            payload: payload.clone().map(lower_mir_operand),
            resume_state: *resume_state,
        },
        MirTerminator::Unreachable => LirTerminator::Unreachable,
    };

    LirBlock { id: block.id, label: block.label.clone(), parameters: block.parameters.clone(), operations, terminator }
}

fn lower_mir_instruction(instruction: &MirInstruction) -> LirOperation {
    let kind = match &instruction.kind {
        MirOperation::LoadConstant { constant, ty } => LirOperationKind::LoadConstant { constant: constant.clone(), ty: ty.clone() },
        MirOperation::LoadSymbol { path } => LirOperationKind::LoadSymbol { path: path.clone() },
        MirOperation::Copy { source } => LirOperationKind::Move { source: lower_mir_operand(source.clone()) },
        MirOperation::StoreVar { name, value, ty } => {
            LirOperationKind::StoreVar { name: name.clone(), value: lower_mir_operand(value.clone()), ty: ty.clone() }
        }
        MirOperation::Call { callee, arguments } => LirOperationKind::Call {
            callee: lower_mir_operand(callee.clone()),
            arguments: arguments.iter().cloned().map(lower_mir_operand).collect(),
        },
        // Legacy LIR maps: keep shapes until LIR dies. Semantic MIR no longer has FixedArrayNew/ArrayLiteral.
        MirOperation::ArrayNew { array_type, length, .. } => {
            LirOperationKind::ArrayNew { element_type: array_type.clone(), length: lower_mir_operand(length.clone()) }
        }
        MirOperation::ArrayFromElements { array_type, elements } => LirOperationKind::ArrayFromElements {
            element_type: array_type.clone(),
            items: elements.iter().cloned().map(lower_mir_operand).collect(),
        },
        MirOperation::ArrayGet { .. } | MirOperation::ArraySet { .. } | MirOperation::ArrayLength { .. } => {
            LirOperationKind::Move { source: LirOperand::Constant(MirConstant::Unit) }
        }
        MirOperation::StructNew { type_name, fields } => LirOperationKind::StructNew {
            type_name: type_name.clone(),
            storage: crate::valkyrie::mir::MirStorageKind::Value,
            fields: fields.iter().map(|(name, value)| (name.clone(), lower_mir_operand(value.clone()))).collect(),
        },
        MirOperation::TupleNew { fields, .. } => LirOperationKind::StructNew {
            type_name: format!("Tuple{}", fields.len()),
            storage: crate::valkyrie::mir::MirStorageKind::Value,
            fields: fields.iter().enumerate().map(|(index, value)| (index.to_string(), lower_mir_operand(value.clone()))).collect(),
        },
        MirOperation::AggregateCopy { source, .. } => LirOperationKind::Move { source: lower_mir_operand(source.clone()) },
        MirOperation::FieldGet { object, field } => {
            LirOperationKind::FieldGet { object: lower_mir_operand(object.clone()), field: field.clone() }
        }
        MirOperation::FieldSet { object, field, value } => LirOperationKind::FieldSet {
            object: lower_mir_operand(object.clone()),
            field: field.clone(),
            value: lower_mir_operand(value.clone()),
        },
        // Legacy LIR is not a backend input. Preserve the payload dependency;
        // nominal construction remains exclusively Semantic MIR.
        MirOperation::SumNew { payload, .. } => payload
            .clone()
            .map(lower_mir_operand)
            .map_or(LirOperationKind::Move { source: LirOperand::Constant(MirConstant::Unit) }, |source| LirOperationKind::Move { source }),
        // Legacy LIR is not a backend input. Preserve only the SSA use here;
        // nominal sum semantics remain represented by Semantic MIR.
        MirOperation::SumPayloadGet { object, .. } => LirOperationKind::Move { source: lower_mir_operand(object.clone()) },
        MirOperation::SumVariantIs { object, .. } => LirOperationKind::Move { source: lower_mir_operand(object.clone()) },
        // Legacy LIR is not a backend input. Keep the data dependency only;
        // encoding conversion semantics remain exclusively in Semantic MIR.
        MirOperation::PatternMatch { value, pattern } => {
            LirOperationKind::PatternMatch { value: lower_mir_operand(value.clone()), pattern: pattern.clone() }
        }
    };

    LirOperation { output: instruction.results.first().copied(), kind }
}

fn lower_mir_operand(operand: MirOperand) -> LirOperand {
    match operand {
        MirOperand::Value(value) => LirOperand::Value(value),
        MirOperand::Constant(constant) => LirOperand::Constant(constant),
        MirOperand::Symbol(path) => LirOperand::Symbol(path),
    }
}

/// Preserves the upstream effect category when lowering `MIR` terminators into `LIR`.
pub fn lower_effect_kind(effect: MirEffectKind) -> LirEffectKind {
    match effect {
        MirEffectKind::Raise => LirEffectKind::Raise,
        MirEffectKind::Yield => LirEffectKind::Yield,
        MirEffectKind::DelegateYield => LirEffectKind::DelegateYield,
        MirEffectKind::Await => LirEffectKind::Await,
        MirEffectKind::AsyncSpawn => LirEffectKind::AsyncSpawn,
        MirEffectKind::AsyncBlock => LirEffectKind::AsyncBlock,
    }
}
