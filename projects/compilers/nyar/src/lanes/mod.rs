#![doc = include_str!("readme.md")]

use serde::{Deserialize, Serialize};

use crate::{
    abstractions::{BackendInputKind, BinaryTarget},
    packaging::TargetLane,
};

/// lane 描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetLoweringLaneDescriptor {
    /// lane 名。
    pub name: String,
    /// 所属路线。
    pub lane: TargetLane,
    /// 产出的输入种类。
    pub input_kind: BackendInputKind,
    /// 面向的目标。
    pub target: BinaryTarget,
}

/// lane lowering 结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneLoweringResult<TInput> {
    /// 产出的 backend input。
    pub input: TInput,
    /// 产物逻辑名。
    pub artifact_name: String,
}

/// 目标路线承接接口。
///
/// 它只负责任务分发和目标特定低层输入构造，
/// 不负责 trait resolve、row 闭合或 effect handler 选择。
pub trait TargetLoweringLane {
    /// 进入该路线前的分区输入。
    type PartitionInput;
    /// 该路线产出的 backend input。
    type BackendInput;

    /// 返回路线描述。
    fn descriptor(&self) -> &TargetLoweringLaneDescriptor;

    /// 把分区输入降到本路线的后端输入。
    fn lower_partition(&self, partition: Self::PartitionInput) -> miette::Result<LaneLoweringResult<Self::BackendInput>>;
}
