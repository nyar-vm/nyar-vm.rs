//! Driver-agnostic assembled fragment payload shared by frontends and the driver.

use nyar_optimizer::TheoryBundle;
use nyar_types::CompiledProgram;

use crate::planning::ArtifactPartition;

/// Fragment payload produced by a language frontend after MIR lowering.
///
/// The driver receives the verified compiler payload and derives its backend submission view exactly once.
#[derive(Debug, Clone)]
pub struct AssembledFragment {
    /// 编译器已经校验的目标分区合同。
    pub partition: ArtifactPartition,
    /// Optimizer theory selected for this fragment.
    pub theory_bundle: TheoryBundle,
    /// Compiler 产生的完整 canonical/representation 成功载荷。
    pub compiled_program: CompiledProgram,
}
