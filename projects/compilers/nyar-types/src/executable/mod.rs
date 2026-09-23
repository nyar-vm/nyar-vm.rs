//! Canonical Semantic MIR boundary consumed by all backend lowerers.
//!
//! IntrinsicOpcode / Call God fields / StorageKind·LayoutId on instructions are
//! **deleted** (ADR 0010 / 0011). Physical layout belongs in RepresentationPlan.

use std::collections::BTreeMap;

use ordered_float::OrderedFloat;

use crate::{NamePath, NyarType};

/// Tombstone — do not restore IntrinsicOpcode as MIR authority (ADR 0010).
pub mod intrinsic;

/// SSA value reference within an executable function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueRef(pub u32);

/// Basic-block reference within an executable function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockRef(pub u32);

/// Origin of an SSA value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueOrigin {
    /// Function parameter.
    Parameter {
        /// Parameter index.
        index: usize,
        /// Parameter name.
        name: String,
    },
    /// Block parameter.
    BlockParameter {
        /// Owning block.
        block: BlockRef,
        /// Parameter name.
        name: String,
    },
    /// `let` binding.
    LetBinding {
        /// Binding name.
        name: String,
    },
    /// Pattern `mut pat`: mutable borrow binding.
    MutRefBinding {
        /// Binding name.
        name: String,
    },
    /// Pattern `pin mut pat`: pinned mutable borrow binding.
    PinMutRefBinding {
        /// Binding name.
        name: String,
    },
    /// Literal materialization.
    Literal,
    /// Path / symbol load.
    Path,
    /// Call result.
    CallResult,
    /// Compiler temporary.
    Temporary,
}

/// SSA value definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    /// Value id.
    pub id: ValueRef,
    /// How the value was introduced.
    pub origin: ValueOrigin,
}

/// Executable constant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Constant {
    /// Signed integer constant.
    Int(i64),
    /// IEEE-754 float64 constant.
    Float64(OrderedFloat<f64>),
    /// Boolean constant.
    Bool(bool),
    /// UTF-8 text constant. Valkyrie has no unqualified language-level
    /// `string`; the encoding is part of the Semantic MIR contract.
    Utf8(String),
    /// UTF-16 text constant. This remains distinct from [`Self::Utf8`] even
    /// when a backend happens to use one physical carrier for both forms.
    Utf16(String),
    /// Unit constant.
    Unit,
}

/// Instruction / terminator operand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    /// SSA value.
    Value(ValueRef),
    /// Immediate constant.
    Constant(Constant),
    /// Named symbol path.
    Symbol(NamePath),
}

/// Effect terminator category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
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

/// Executable instruction (thin mirror of Semantic MIR envelope; ADR 0012).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    /// Stable instruction identity.
    pub id: crate::InstructionId,
    /// SSA results (0..n); types live in the value table.
    pub results: Vec<ValueRef>,
    /// Instruction payload (transitional name; target MirOperation).
    pub kind: InstructionKind,
    /// Source or synthetic provenance.
    pub provenance: crate::ProvenanceId,
}

impl Instruction {
    /// Wrap a kind in a synthetic envelope (tests / transitional emitters).
    pub fn from_kind(kind: InstructionKind) -> Self {
        Self {
            id: crate::InstructionId::from_index(0).expect("stub InstructionId"),
            results: Vec::new(),
            kind,
            provenance: crate::ProvenanceId::from_index(0).expect("synthetic ProvenanceId"),
        }
    }
}

/// Executable instruction kinds (MIR-shaped, platform types only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstructionKind {
    /// Materialize a constant.
    LoadConstant {
        /// Constant payload.
        constant: Constant,
        /// Optional static type.
        ty: Option<NyarType>,
    },
    /// Load a named symbol.
    LoadSymbol {
        /// Symbol path.
        path: NamePath,
    },
    /// Copy an operand into the output.
    Copy {
        /// Source operand.
        source: Operand,
    },
    /// Store into a named local (`let` binding).
    StoreVar {
        /// Variable name.
        name: String,
        /// Value to store.
        value: Operand,
        /// Optional static type.
        ty: Option<NyarType>,
    },
    /// Call a callee (transitional LegacyCall shape → ADR 0010 `Invoke`).
    ///
    /// **Removed God fields (do not reintroduce):** `dispatch`, `witness`, `evidence`,
    /// `generic_function`, `generic_arguments`, `effect`, `receiver_kind`,
    /// `parameter_types`, `intrinsic_opcode`, `signature_complete`, `has_this`.
    Call {
        /// Callable operand (`ItemInstanceId` / function value TBD).
        callee: Operand,
        /// Arguments in semantic order.
        arguments: Vec<Operand>,
    },
    /// Construct a named aggregate / struct instance.
    StructNew {
        /// Type name.
        type_name: String,
        /// Field initializers `(name, value)`.
        fields: Vec<(String, Operand)>,
    },
    /// Construct a tuple.
    TupleNew {
        /// Element values. Arity/types come from result `TypeId` (ADR 0011).
        fields: Vec<Operand>,
    },
    /// Copy an aggregate by layout.
    AggregateCopy {
        /// Source aggregate.
        source: Operand,
        /// Destination aggregate.
        dest: Operand,
    },
    /// Read a field.
    FieldGet {
        /// Object / aggregate.
        object: Operand,
        /// Field name.
        field: String,
    },
    /// Write a field.
    FieldSet {
        /// Object / aggregate.
        object: Operand,
        /// Field name.
        field: String,
        /// Value to write.
        value: Operand,
    },
    /// Construct a value of an explicitly declared nominal sum variant.
    SumNew {
        /// Sum registry identity (nominal name).
        sum_type: String,
        /// Type arguments forming `NominalInstanceKey` with `sum_type` (empty ⇒ monomorphic).
        type_args: Vec<NyarType>,
        /// Declared variant identity.
        variant: String,
        /// Payload type declared by the variant, when it carries one.
        payload_type: Option<NyarType>,
        /// Payload value for a payload-bearing variant.
        payload: Option<Operand>,
    },
    /// Extract a payload from a declared nominal sum variant.
    ///
    /// This is distinct from aggregate field access: the sum and variant
    /// identity are semantic metadata, never inferred from field spellings.
    SumPayloadGet {
        /// Sum registry name.
        sum_type: String,
        /// Type arguments forming `NominalInstanceKey` with `sum_type` (empty ⇒ monomorphic).
        type_args: Vec<NyarType>,
        /// Active variant known by the surrounding structured control flow.
        variant: String,
        /// Declared payload type for that variant.
        payload_type: NyarType,
        /// Sum receiver.
        object: Operand,
    },
    /// Test whether a sum value is currently the given declared variant.
    ///
    /// Carries the same `NominalInstanceKey` as [`Self::SumNew`] / [`Self::SumPayloadGet`].
    /// Must not be lowered as a field named `tag` discovered from a physical carrier.
    SumVariantIs {
        /// Sum registry name.
        sum_type: String,
        /// Type arguments forming `NominalInstanceKey` with `sum_type`.
        type_args: Vec<NyarType>,
        /// Declared variant identity.
        variant: String,
        /// Sum receiver.
        object: Operand,
    },
    /// Pattern probe for handler / case dispatch.
    PatternMatch {
        /// Scrutinee value.
        value: Operand,
        /// Debug-only pattern description (not a language HIR pattern).
        pattern_debug: String,
    },
    /// Runtime-length language array construction (ADR 0011). Full `array_type`, not element-only.
    ArrayNew {
        /// Full array type (`Array<T>` / …); transitional stand-in for `TypeId`.
        array_type: NyarType,
        /// Runtime length.
        length: Operand,
        /// Prescribed initialization (no silent “fill later”).
        initialization: ArrayInitialization,
    },
    /// Construct language array from a complete element sequence (replaces FixedArrayNew/ArrayLiteral).
    ArrayFromElements {
        /// Full array type; fixed length lives in the type, not a parallel `usize`.
        array_type: NyarType,
        /// Element values in evaluation order already sequenced by SSA/CFG.
        elements: Vec<Operand>,
    },
    ArrayGet {
        array: Operand,
        index: Operand,
    },
    ArraySet {
        array: Operand,
        index: Operand,
        value: Operand,
    },
    ArrayLength {
        array: Operand,
    },
}

/// Array slot initialization for [`InstructionKind::ArrayNew`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrayInitialization {
    /// Language-defined default for the element type.
    Default,
    /// Fill every slot with this value.
    Fill(Operand),
}

/// Basic-block terminator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminator {
    /// Return from the function.
    Return {
        /// Optional return value.
        value: Option<Operand>,
    },
    /// Unconditional jump with block arguments.
    Jump {
        /// Target block.
        target: BlockRef,
        /// Block arguments.
        arguments: Vec<Operand>,
    },
    /// Conditional branch.
    Branch {
        /// Branch condition.
        condition: Operand,
        /// Then target.
        then_target: BlockRef,
        /// Else target.
        else_target: BlockRef,
    },
    /// Perform an effect and resume at `resume_target`.
    PerformEffect {
        /// Effect category.
        effect: EffectKind,
        /// Optional payload.
        payload: Option<Operand>,
        /// Resume target.
        resume_target: BlockRef,
    },
    /// State-machine dispatch on a state value.
    StateDispatch {
        /// State SSA value.
        state: ValueRef,
        /// `(state_id, target)` cases.
        cases: Vec<(u32, BlockRef)>,
        /// Default target.
        default_target: BlockRef,
    },
    /// Yield control to runtime with a resume state.
    YieldToRuntime {
        /// Effect category.
        effect: EffectKind,
        /// Optional payload.
        payload: Option<Operand>,
        /// Resume state id.
        resume_state: u32,
    },
    /// Unreachable terminator.
    Unreachable,
}

/// Continuation metadata for catch / resume contexts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Continuation {
    /// Handler dispatch block.
    pub dispatch_block: BlockRef,
    /// Resume target block.
    pub resume_target: BlockRef,
    /// Resume parameter SSA value.
    pub resume_parameter: ValueRef,
    /// Resume parameter static type, if known.
    pub resume_parameter_type: Option<NyarType>,
    /// Handler exit block.
    pub handler_exit: BlockRef,
    /// User effect carrier type (Raise only), if known.
    pub carrier_type: Option<NyarType>,
}

/// `match` / `case` chain metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseChain {
    /// Dispatch block entering the chain.
    pub dispatch_block: BlockRef,
    /// First arm entry.
    pub first_arm: BlockRef,
    /// Block entered when no arm matches.
    pub no_match_block: BlockRef,
    /// Final exit block.
    pub exit_block: BlockRef,
    /// Whether this chain produces a value.
    pub produce_value: bool,
    /// Per-arm metadata.
    pub arms: Vec<CaseArm>,
}

/// Single case / match arm control-flow metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseArm {
    /// Arm entry block.
    pub entry_block: BlockRef,
    /// Optional pattern-check block.
    pub check_block: Option<BlockRef>,
    /// Optional guard block.
    pub guard_block: Option<BlockRef>,
    /// Body block.
    pub body_block: BlockRef,
    /// Next target on pattern / guard failure.
    pub next_arm_target: BlockRef,
    /// Exit target after a successful arm.
    pub exit_target: BlockRef,
    /// Optional fallthrough target.
    pub fallthrough_target: Option<BlockRef>,
}

/// Suspend-point metadata for effect / state-machine lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuspendPoint {
    /// State-machine id.
    pub state_id: u32,
    /// Effect category.
    pub effect: EffectKind,
    /// Suspend block.
    pub suspend_block: BlockRef,
    /// Resume target.
    pub resume_target: BlockRef,
    /// Resume parameter count.
    pub resume_parameter_count: usize,
    /// Payload static type, if known.
    pub payload_type: Option<NyarType>,
    /// Spill candidates across the suspend.
    pub spill_candidates: Vec<ValueRef>,
    /// Optional continuation index when nested in a catch arm.
    pub continuation_index: Option<usize>,
    /// User effect carrier type (Raise only), if known.
    pub carrier_type: Option<NyarType>,
}

/// Frame layout for one suspend state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameLayout {
    /// State-machine id.
    pub state_id: u32,
    /// Effect category.
    pub effect: EffectKind,
    /// Resume target.
    pub resume_target: BlockRef,
    /// Spill slots.
    pub slots: Vec<FrameSlot>,
    /// User effect carrier type (Raise only), if known.
    pub carrier_type: Option<NyarType>,
}

/// One spill slot inside a frame layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameSlot {
    /// Stable slot index.
    pub slot_index: usize,
    /// Spilled SSA value.
    pub value: ValueRef,
    /// Known static type, if any.
    pub value_type: Option<NyarType>,
}

/// Function-level frame / continuation carrier naming table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarrierTable {
    function_symbol: String,
}

impl CarrierTable {
    /// Builds a carrier naming table for a suspend function.
    pub fn new(function_symbol: impl Into<String>) -> Self {
        Self { function_symbol: function_symbol.into() }
    }

    /// Frame carrier name (lane-neutral).
    pub fn frame(&self, state_id: u32) -> String {
        format!("{}$state_{state_id}_frame", self.function_symbol)
    }

    /// Continuation carrier name.
    pub fn continuation(&self, index: usize) -> String {
        format!("{}$continuation_{index}", self.function_symbol)
    }

    /// Continuation resume-value field name.
    pub fn continuation_resume_field() -> &'static str {
        "resume_value"
    }

    /// Runtime frame slot field name.
    pub fn frame_slot_field(slot_index: usize) -> String {
        format!("slot_{slot_index}")
    }
}

/// One suspend state entry in a lowering plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuspendState {
    /// State-machine id.
    pub state_id: u32,
    /// Effect category.
    pub effect: EffectKind,
    /// Suspend block.
    pub suspend_block: BlockRef,
    /// Resume target.
    pub resume_target: BlockRef,
    /// Resume parameter count.
    pub resume_parameter_count: usize,
    /// Resume parameter static type, if known.
    pub resume_parameter_type: Option<NyarType>,
    /// Payload static type, if known.
    pub payload_type: Option<NyarType>,
    /// Spill slots.
    pub spill_slots: Vec<ValueRef>,
    /// Frame carrier name.
    pub frame_carrier: String,
    /// Optional continuation index.
    pub continuation_index: Option<usize>,
}

/// Function-level suspend lowering plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuspendLoweringPlan {
    /// Function symbol.
    pub function_symbol: String,
    /// Entry block.
    pub entry_block: BlockRef,
    /// Suspend states.
    pub states: Vec<SuspendState>,
    /// Handler dispatch blocks.
    pub handler_dispatch_blocks: Vec<BlockRef>,
    /// Carrier naming table.
    pub carrier_table: CarrierTable,
}

/// Compile-time diagnostic collected while building an executable view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagnostic {
    /// Pattern matching could not be lowered.
    PatternLoweringFailed {
        /// Human-readable reason.
        reason: String,
    },
}

/// Executable basic block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Block id.
    pub id: BlockRef,
    /// Debug label.
    pub label: String,
    /// Block parameters.
    pub parameters: Vec<ValueRef>,
    /// Instructions.
    pub instructions: Vec<Instruction>,
    /// Terminator.
    pub terminator: Terminator,
}

/// Backend-private executable function payload.
///
/// MIR-shaped during migration, but not a platform god IR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableFunction {
    /// Function symbol.
    pub symbol: String,
    /// Return type.
    pub return_type: NyarType,
    /// Parameter types.
    pub param_types: Vec<NyarType>,
    /// SSA value static types.
    pub value_types: BTreeMap<ValueRef, NyarType>,
    /// Entry block.
    pub entry: BlockRef,
    /// SSA values.
    pub values: Vec<Value>,
    /// Suspend points.
    pub suspend_points: Vec<SuspendPoint>,
    /// Frame layouts.
    pub frame_layouts: Vec<FrameLayout>,
    /// Continuations.
    pub continuations: Vec<Continuation>,
    /// Case / match chains.
    pub case_chains: Vec<CaseChain>,
    /// Deprecated alias of [`Self::suspend_plan`].
    #[deprecated(note = "use suspend_plan")]
    pub state_machine: Option<SuspendLoweringPlan>,
    /// Explicit suspend lowering plan.
    pub suspend_plan: Option<SuspendLoweringPlan>,
    // DELETED (ADR 0011): state_machine_lowered parallel CFG-rewrite flag.
    /// Blocks.
    pub blocks: Vec<Block>,
    /// Diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}
