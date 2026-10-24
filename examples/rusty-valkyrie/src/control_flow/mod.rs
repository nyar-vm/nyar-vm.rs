//! Unified control-flow scope and label tracking for HIR validation and MIR lowering.

mod context;

pub use context::{
    AsyncScopeData, CaseChainScopeData, CatchScopeData, ControlFlowContext, ControlFlowScopeKind, GeneratorScopeData, LoopScopeData, ScopeKind,
    TryScopeData,
};
