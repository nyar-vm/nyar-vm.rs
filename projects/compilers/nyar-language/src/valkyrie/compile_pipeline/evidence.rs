//! Compiler 成功路径的可复现证据（digest / provenance）。

use nyar_types::{CompiledProgram, combined_content_hash};

use crate::valkyrie::{CompilerSourceGroup, GeneratedSourceGroup};

use super::context::CompilerHostProviderBinding;

/// 从源码闭包到 Canonical 表面的稳定编译证据。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CompilerCompileEvidence {
    /// Resolver 提交的语义源码闭包摘要。
    pub semantic_closure_hash: String,
    /// 已验证 `CompiledProgram` 的导出 / 入口 / 函数表面摘要。
    pub canonical_surface_hash: String,
    /// Resolver 选定的 host provider 绑定摘要。
    pub host_provider_binding_hash: String,
    /// 独立生成源码的 producer / 源码摘要；无生成输入时为空串。
    #[cfg_attr(feature = "serde", serde(default))]
    pub generated_source_hash: String,
}

impl CompilerCompileEvidence {
    /// 从应用源码组、成功载荷与 host 绑定构造证据（无生成源码）。
    ///
    /// `arch` 必须与本次编译使用的模板展开键一致，否则闭包摘要无法与证据比对。
    pub fn from_success(
        groups: &[CompilerSourceGroup],
        arch: &str,
        program: &CompiledProgram,
        host_bindings: &[CompilerHostProviderBinding],
    ) -> Self {
        Self::from_success_with_generated(groups, &[], arch, program, host_bindings)
    }

    /// 从已合成的源码组、独立生成源码、成功载荷与 host 绑定构造证据。
    ///
    /// `groups` 必须是 Compiler 实际编译的合成闭包；生成源码只作为独立 provenance
    /// 进入证据，不会被再次拼接或重解析。
    pub fn from_success_with_generated(
        groups: &[CompilerSourceGroup],
        generated: &[GeneratedSourceGroup],
        arch: &str,
        program: &CompiledProgram,
        host_bindings: &[CompilerHostProviderBinding],
    ) -> Self {
        Self {
            semantic_closure_hash: semantic_closure_hash(groups, arch),
            canonical_surface_hash: program.surface_digest(),
            host_provider_binding_hash: host_provider_binding_hash(host_bindings),
            generated_source_hash: generated_source_hash(generated),
        }
    }
}

/// 从 Resolver 提交的源码组计算语义闭包摘要（供 freshness 比对）；`arch` 与编译上下文一致。
pub fn semantic_closure_hash_for_groups(groups: &[CompilerSourceGroup], arch: &str) -> String {
    semantic_closure_hash(groups, arch)
}

/// 从独立生成源码组计算来源摘要；无生成输入时为空串。
pub fn generated_source_hash(generated: &[GeneratedSourceGroup]) -> String {
    if generated.is_empty() {
        return String::new();
    }
    let mut parts = Vec::new();
    for generated in generated {
        parts.push(format!("producer:{}", generated.producer));
        parts.push(format!("group:{}", generated.group.dependency_key));
        for dependency in &generated.group.direct_dependencies {
            parts.push(format!("dependency:{dependency}"));
        }
        for file in &generated.group.sources {
            parts.push(format!("file:{}", file.path));
            parts.push(format!("content:{}", combined_content_hash(&[file.source.as_str()])));
        }
    }
    let borrowed = parts.iter().map(String::as_str).collect::<Vec<_>>();
    combined_content_hash(&borrowed)
}

/// 从 host provider 绑定计算摘要（供 freshness 比对）。
pub fn host_provider_binding_hash_for_bindings(host_bindings: &[CompilerHostProviderBinding]) -> String {
    host_provider_binding_hash(host_bindings)
}

fn semantic_closure_hash(groups: &[CompilerSourceGroup], arch: &str) -> String {
    // 闭包和摘要使用同一 Compiler 生产者，不能信任调用方提交的摘要。
    let digest = match crate::valkyrie::semantic_closure::source_group_closure(groups, arch) {
        Ok(closure) => closure.digest,
        // 不可规划的入口不是一次成功闭包：摘要显式记录失败事实，供 freshness 区分。
        Err(error) => format!("closure-plan-error:{error}"),
    };
    let mut parts = vec![digest];
    for group in groups {
        parts.push(group.dependency_key.clone());
        parts.push(group.name.clone());
        for dependency in &group.direct_dependencies {
            parts.push(dependency.clone());
        }
    }
    let borrowed = parts.iter().map(String::as_str).collect::<Vec<_>>();
    combined_content_hash(&borrowed)
}

fn host_provider_binding_hash(host_bindings: &[CompilerHostProviderBinding]) -> String {
    let mut parts = Vec::new();
    for binding in host_bindings {
        parts.push(binding.contract.encoded());
        parts.push(binding.provider.encoded());
    }
    let borrowed = parts.iter().map(String::as_str).collect::<Vec<_>>();
    combined_content_hash(&borrowed)
}
