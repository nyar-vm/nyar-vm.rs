//! 编译器与驱动共享的显式目标分区载荷。

use nyar_optimizer::TheoryBundle;
use nyar_types::CompiledProgram;

use crate::planning::ArtifactPartition;

/// 编译器在语义与表示验证之后提交的目标分区。
/// 驱动不得重新生产调用、类型或布局事实。
#[derive(Debug, Clone)]
pub struct AssembledFragment {
    /// 编译器已经校验的目标分区合同。
    pub partition: ArtifactPartition,
    /// 分区规划选定的优化理论。
    pub theory_bundle: TheoryBundle,
    /// Compiler 产生的完整 canonical/representation 成功载荷。
    pub compiled_program: CompiledProgram,
}
