//! GPU semantic fragment detection and rewrite theory wiring.

use nyar::{
    CapabilityTag, Identifier, ObjectAlgebraicDimension, QualifiedName, RewriteTheory, SemanticFragment, builtin_graphic_manifest,
    builtin_neural_manifest,
};

use crate::valkyrie::types::hir::{HirAttribute, HirFunction, HirModule};

/// 检测模块是否包含 graphic（shader）片段。
pub fn module_has_graphic_fragment(module: &HirModule) -> bool {
    module.functions.iter().any(is_graphic_function)
}

/// 检测模块是否包含 neural 片段。
pub fn module_has_neural_fragment(module: &HirModule) -> bool {
    module.functions.iter().any(is_neural_function)
}

fn is_graphic_function(function: &HirFunction) -> bool {
    has_gpu_annotation(&function.annotations, &["gpu_shader", "shader", "graphic"])
        || function.name.as_str().ends_with("_vs")
        || function.name.as_str().ends_with("_fs")
        || function.name.as_str().ends_with("_cs")
}

fn is_neural_function(function: &HirFunction) -> bool {
    has_gpu_annotation(&function.annotations, &["gpu_compute", "neural", "tensor"])
}

fn has_gpu_annotation(annotations: &[HirAttribute], tags: &[&str]) -> bool {
    annotations.iter().any(|attribute| {
        let name = attribute.name.parts().last().map(|part| part.as_str()).unwrap_or_default();
        tags.iter().any(|tag| name == *tag)
    })
}

/// 追加 graphic / neural OA 维度。
pub fn append_gpu_dimensions(module: &HirModule, dimensions: &mut Vec<ObjectAlgebraicDimension>, module_name: &QualifiedName) {
    if module_has_graphic_fragment(module) {
        let ops = module
            .functions
            .iter()
            .filter(|function| is_graphic_function(function))
            .map(|function| function_symbol(module_name, function))
            .collect();
        dimensions.push(ObjectAlgebraicDimension {
            name: Identifier::new("graphic"),
            exported_operations: ops,
            required_capabilities: vec![CapabilityTag::new("gpu-shader")],
            reference_management_hint: None,
        });
    }
    if module_has_neural_fragment(module) {
        let ops = module
            .functions
            .iter()
            .filter(|function| is_neural_function(function))
            .map(|function| function_symbol(module_name, function))
            .collect();
        dimensions.push(ObjectAlgebraicDimension {
            name: Identifier::new("neural"),
            exported_operations: ops,
            required_capabilities: vec![CapabilityTag::new("gpu-compute")],
            reference_management_hint: None,
        });
    }
}

fn function_symbol(module_name: &QualifiedName, function: &HirFunction) -> QualifiedName {
    let mut parts = module_name.parts().to_vec();
    if parts.is_empty() {
        parts.push(Identifier::new("app"));
    }
    parts.push(function.name.clone());
    QualifiedName::new(parts)
}

/// 为 semantic fragment 填充片段级 rewrite theory。
pub fn rewrite_theory_for_fragment(fragment_id: &str) -> RewriteTheory {
    match fragment_id {
        "graphic" => builtin_graphic_manifest().to_rewrite_theory(),
        "neural" => builtin_neural_manifest().to_rewrite_theory(),
        _ => RewriteTheory::default(),
    }
}

/// 将 GPU 片段注入 semantic_fragments 列表（若尚未存在）。
pub fn ensure_gpu_semantic_fragments(module: &HirModule, module_name: &QualifiedName, fragments: &mut Vec<SemanticFragment>) {
    if module_has_graphic_fragment(module) && !fragments.iter().any(|fragment| fragment.id.as_str() == "graphic") {
        fragments.push(SemanticFragment {
            id: Identifier::new("graphic"),
            exported_operations: module
                .functions
                .iter()
                .filter(|function| is_graphic_function(function))
                .map(|function| function_symbol(module_name, function))
                .collect(),
            required_capabilities: vec![CapabilityTag::new("gpu-shader")],
            reference_management_hint: None,
            entry_operation: None,
            external_import_links: Default::default(),
            external_call_edges: Vec::new(),
            internal_call_edges: Vec::new(),
            operation_literal_returns: Default::default(),
            operation_void_returns: Default::default(),
            witness_tables: Vec::new(),
            witness_calls: Vec::new(),
            rewrite_theory: rewrite_theory_for_fragment("graphic"),
        });
    }
    if module_has_neural_fragment(module) && !fragments.iter().any(|fragment| fragment.id.as_str() == "neural") {
        fragments.push(SemanticFragment {
            id: Identifier::new("neural"),
            exported_operations: module
                .functions
                .iter()
                .filter(|function| is_neural_function(function))
                .map(|function| function_symbol(module_name, function))
                .collect(),
            required_capabilities: vec![CapabilityTag::new("gpu-compute")],
            reference_management_hint: None,
            entry_operation: None,
            external_import_links: Default::default(),
            external_call_edges: Vec::new(),
            internal_call_edges: Vec::new(),
            operation_literal_returns: Default::default(),
            operation_void_returns: Default::default(),
            witness_tables: Vec::new(),
            witness_calls: Vec::new(),
            rewrite_theory: rewrite_theory_for_fragment("neural"),
        });
    }
}
