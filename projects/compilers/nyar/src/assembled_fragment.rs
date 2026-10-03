//! Driver-agnostic assembled fragment payload shared by frontends and the driver.

use nyar_optimizer::TheoryBundle;
use nyar_types::{CompiledProgram, Identifier, ItemInstanceId};

/// Fragment payload produced by a language frontend after MIR lowering.
///
/// The driver receives the verified compiler payload and derives its backend submission view exactly once.
#[derive(Debug, Clone)]
pub struct AssembledFragment {
    /// Logical module name.
    pub module_name: String,
    /// Semantic fragment identifier.
    pub fragment_id: Identifier,
    /// Optimizer theory selected for this fragment.
    pub theory_bundle: TheoryBundle,
    /// Compiler-resolved callable roots for backend closure construction.
    pub callable_roots: Vec<ItemInstanceId>,
    /// Compiler 产生的完整 canonical/representation 成功载荷。
    pub compiled_program: CompiledProgram,
}
