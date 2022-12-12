#![doc = include_str!("readme.md")]

use serde::{Deserialize, Serialize};

use crate::abstractions::{ArtifactFormat, ArtifactKind, BackendInputKind, BinaryTarget};

/// 目标 lane。
///
/// 它表达“走哪条后端路线”，不是统一低层 `IR`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetLane {
    /// `CLR`
    Clr,
    /// `JVM`
    Jvm,
    /// `WASM`
    Wasm,
    /// `native`
    Native,
    /// `CPU/VM`
    Vm,
    /// `GPU / Shader`
    Gpu,
}

/// 单个产物描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactDescriptor {
    /// 文件逻辑名。
    pub name: String,
    /// 产物种类。
    pub kind: ArtifactKind,
    /// 文件格式。
    pub format: ArtifactFormat,
    /// 目标。
    pub target: BinaryTarget,
    /// 生成该产物的 lane。
    pub lane: TargetLane,
}

/// 后端输出规范。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputSpec {
    /// 后端输入种类。
    pub input_kind: BackendInputKind,
    /// 主产物。
    pub primary: ArtifactDescriptor,
    /// 辅助产物。
    pub sidecars: Vec<ArtifactDescriptor>,
}

/// 产物集合。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ArtifactSet {
    /// 全部产物描述。
    pub artifacts: Vec<ArtifactDescriptor>,
}

impl ArtifactSet {
    /// 追加产物描述。
    pub fn push(&mut self, artifact: ArtifactDescriptor) {
        self.artifacts.push(artifact);
    }
}
