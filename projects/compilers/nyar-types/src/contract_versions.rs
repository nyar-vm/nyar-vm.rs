//! 语义合同与产物版本号（S-W6）。
//!
//! 这些常量进入 cache key / execution manifest，旧产物在版本漂移时直接失效。
//! 只允许在合同破坏性变更时递增；不得为兼容旧产物而静默忽略。

/// 语义 identity 合同版本（`ItemInstanceId` / registry / 禁止字符串分派）。
pub const IDENTITY_SCHEMA_VERSION: u32 = 1;

/// Semantic MIR 合同版本（调用 / 类型 / 布局侧表形状）。
pub const MIR_CONTRACT_VERSION: u32 = 3;

/// RepresentationPlan / layout 侧表合同版本。
pub const LAYOUT_PLAN_VERSION: u32 = 1;

/// 将四元组格式化为 cache / provenance 指纹片段。
pub fn contract_version_fingerprint() -> String {
    format!(
        "identity={IDENTITY_SCHEMA_VERSION};mir={MIR_CONTRACT_VERSION};layout={LAYOUT_PLAN_VERSION}"
    )
}
