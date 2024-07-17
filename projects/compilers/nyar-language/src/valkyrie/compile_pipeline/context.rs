//! Compiler 的唯一装配输入合同。
//!
//! Resolver 必须把 target/profile、架构键和已选 host provider 一并交给 Compiler；
//! 这些字段是后续 adaptor 链接与表示规划的前置事实，不能留在源码行扫描里。

use nyar::{CanonicalTarget, ClrSuspendStrategy};
use nyar_emitter::nyar_backend_wasi::WasmPackageKind;

use crate::valkyrie::semantic_closure::DeclarationKey;

/// Resolver 为某个 `[host_provider(...)]` 合同选定的实现绑定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerHostProviderBinding {
    /// `[host_contract]` 的冻结声明身份。
    pub contract: DeclarationKey,
    /// 选定 `[host_provider]` 实现的冻结声明身份。
    pub provider: DeclarationKey,
}

/// Legion 交给 Compiler 的完整构建上下文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerBuildContext {
    /// 目标架构键；仅供 adaptor/模板合同使用，不得触发 Compiler 内文本重解析。
    pub arch: String,
    /// 规范化编译目标（wasm、native、CLR 等）。
    pub target: CanonicalTarget,
    /// CLR 后端挂起/恢复策略。
    pub clr_suspend_strategy: ClrSuspendStrategy,
    /// Wasm 产物形态（模块或组件）。
    pub wasm_package_kind: WasmPackageKind,
    /// 装配层已按 target/profile 过滤的 host provider 选择结果。
    pub selected_host_providers: Vec<CompilerHostProviderBinding>,
}

impl CompilerBuildContext {
    /// 构造最小构建上下文；host provider 列表默认为空。
    pub fn new(
        arch: impl Into<String>,
        target: CanonicalTarget,
        clr_suspend_strategy: ClrSuspendStrategy,
        wasm_package_kind: WasmPackageKind,
    ) -> Self {
        Self { arch: arch.into(), target, clr_suspend_strategy, wasm_package_kind, selected_host_providers: Vec::new() }
    }

    /// 附加 Resolver 已选定的 host provider 绑定。
    pub fn with_selected_host_providers(mut self, providers: Vec<CompilerHostProviderBinding>) -> Self {
        self.selected_host_providers = providers;
        self
    }
}
