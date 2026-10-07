//! Compiler 成功路径的可复现证据（digest / provenance）。

use nyar_types::{combined_content_hash, CompiledProgram};

use crate::CompilerSourceGroup;

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
}

impl CompilerCompileEvidence {
    /// 从源码组、成功载荷与 host 绑定构造证据。
    pub fn from_success(
        groups: &[CompilerSourceGroup],
        program: &CompiledProgram,
        host_bindings: &[CompilerHostProviderBinding],
    ) -> Self {
        Self {
            semantic_closure_hash: semantic_closure_hash(groups),
            canonical_surface_hash: program.surface_digest(),
            host_provider_binding_hash: host_provider_binding_hash(host_bindings),
        }
    }
}

fn semantic_closure_hash(groups: &[CompilerSourceGroup]) -> String {
    let mut parts = Vec::new();
    for group in groups {
        parts.push(group.dependency_key.clone());
        parts.push(combined_content_hash(&[&group.source]));
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
        parts.push(binding.contract.clone());
        parts.push(binding.symbol.clone());
    }
    let borrowed = parts.iter().map(String::as_str).collect::<Vec<_>>();
    combined_content_hash(&borrowed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nyar::{CanonicalTarget, ClrSuspendStrategy};
    use crate::valkyrie::compile_pipeline::{CompilerBuildContext, compile_source_groups_to_artifacts};
    use crate::{CompilerSourceGroup, ValkyrieCompiler};
    use nyar_emitter::nyar_backend_wasi::WasmPackageKind;

    #[test]
    fn compile_evidence_is_stable_for_the_same_source_closure() {
        let groups = [CompilerSourceGroup {
            dependency_key: "app".into(),
            name: "app".into(),
            source: "[main] micro main() -> i32 { return 23 }".into(),
            direct_dependencies: Vec::new(),
        }];
        let context = CompilerBuildContext::new("wasm32", CanonicalTarget::parse("node").expect("node"), ClrSuspendStrategy::default(), WasmPackageKind::Binary);
        let dir = std::env::temp_dir().join("nyar_compile_evidence_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp output");
        let first = compile_source_groups_to_artifacts(&ValkyrieCompiler::default(), &groups, &context, &dir, "app", false, false)
            .expect("first compile");
        let second = compile_source_groups_to_artifacts(&ValkyrieCompiler::default(), &groups, &context, &dir, "app", false, false)
            .expect("second compile");
        assert_eq!(first.evidence, second.evidence);
        assert!(!first.evidence.semantic_closure_hash.is_empty());
        assert!(!first.evidence.canonical_surface_hash.is_empty());
    }
}
