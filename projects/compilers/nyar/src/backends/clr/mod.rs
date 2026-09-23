//! `CLR` 路线后端公共协议。
//!
//! 这里定义 `CLR` 路线共享的产物口味与命名规则，
//! 由具体的 `clr-backend` 去实现真实 lowering 和二进制编码。

use serde::{Deserialize, Serialize};

use crate::{abstractions::ArtifactKind, planning::SuspendConsumptionModel};

/// CLR suspend 消费策略（双路径）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ClrSuspendStrategy {
    /// Standalone PE / 全 effect：编译器 state machine（`ControlFlowPayload`）。
    #[default]
    StateMachine,
    /// .NET 11+ Runtime Async：运行时 first-class（`SuspendRuntimePayload`，await-only）。
    RuntimeAsync,
}

impl ClrSuspendStrategy {
    /// 从 legion build 块 `runtime_async` 字段解析。
    pub fn from_runtime_async_flag(runtime_async: bool) -> Self {
        if runtime_async { Self::RuntimeAsync } else { Self::StateMachine }
    }

    /// 映射到通用 suspend 消费模型。
    pub fn consumption_model(self) -> SuspendConsumptionModel {
        match self {
            Self::StateMachine => SuspendConsumptionModel::StateMachine,
            Self::RuntimeAsync => SuspendConsumptionModel::FirstClass,
        }
    }
}

/// 前端构建上下文（target 相关 suspend 选项）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrontendBuildContext {
    /// CLR suspend 策略；非 CLR target 时忽略。
    pub clr_suspend_strategy: ClrSuspendStrategy,
}

impl FrontendBuildContext {
    /// 由 legion `BuildTargetSpec` 字段构造。
    pub fn from_clr_runtime_async(runtime_async: bool) -> Self {
        Self { clr_suspend_strategy: ClrSuspendStrategy::from_runtime_async_flag(runtime_async) }
    }
}

/// `CLR` 镜像口味。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClrImageKind {
    /// 带托管入口点的可执行镜像。
    Executable,
    /// 不带入口点的托管动态库。
    DynamicLibrary,
}

impl ClrImageKind {
    /// 根据是否存在入口点推断镜像口味。
    pub fn infer(has_entry_point: bool) -> Self {
        if has_entry_point { Self::Executable } else { Self::DynamicLibrary }
    }

    /// 返回对应的产物种类。
    pub fn artifact_kind(self) -> ArtifactKind {
        match self {
            Self::Executable => ArtifactKind::Executable,
            Self::DynamicLibrary => ArtifactKind::DynamicLibrary,
        }
    }

    /// 返回推荐的文件扩展名。
    pub fn file_extension(self) -> &'static str {
        match self {
            Self::Executable => "exe",
            Self::DynamicLibrary => "dll",
        }
    }
}
