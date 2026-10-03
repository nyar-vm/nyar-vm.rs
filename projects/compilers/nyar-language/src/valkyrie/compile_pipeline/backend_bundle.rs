//! Compiler 生成的完整目标编译 bundle。
//!
//! Legion 与其他装配层只能提交源码快照和目标合同；分区规划、fragment
//! assembly 以及目标私有输入都在 Compiler 内完成。

use std::path::Path;

use emitter::{FrontendBuildBundle, LoweredBackendInput, PlannedArtifactPartitionsView};
use miette::{Result, miette};
use nyar::{
    ArtifactPartitionPlan, CanonicalTarget, ClrSuspendStrategy, HostProjectionBoundary, TargetBackendFamily,
    TargetLane, VmSuspendStrategy,
};

use crate::{CompilerSourceGroup, FrontendBuildOutput, ValkyrieCompiler, assemble_fragment, build_output_surface_counts, plan_artifacts_from_build_output};

/// Compiler 已完成语义分析、表示规划和分区装配的目标输入 bundle。
struct CompilerBuildBundle {
    build_output: FrontendBuildOutput,
    artifact_plan: ArtifactPartitionPlan,
    wasm_package_kind: emitter::nyar_backend_wasi::WasmPackageKind,
}

impl CompilerBuildBundle {
    /// 返回已由 Canonical 成功载荷确认的导出和入口数量。
    fn surface_counts(&self) -> (usize, usize) {
        build_output_surface_counts(&self.build_output)
    }

}

/// 从 Resolver 提供的源码组生产唯一目标 bundle。
fn compile_source_groups_to_backend_bundle(
    compiler: &ValkyrieCompiler,
    groups: &[CompilerSourceGroup],
    arch: &str,
    target: CanonicalTarget,
    clr_suspend_strategy: ClrSuspendStrategy,
    wasm_package_kind: emitter::nyar_backend_wasi::WasmPackageKind,
) -> Result<CompilerBuildBundle> {
    let groups = groups
        .iter()
        .cloned()
        .map(|mut group| {
            group.source = preprocess_target_templates(&group.source, arch);
            group
        })
        .collect::<Vec<_>>();
    let build_output = compiler
        .compile_source_groups(&groups)
        .map_err(|error| miette!("Compiler semantic snapshot failed: {error}"))?;
    let artifact_plan = plan_artifacts_from_build_output(&build_output, target, clr_suspend_strategy)
        .map_err(|error| miette!("Compiler representation planning failed: {error:?}"))?;
    let bundle = CompilerBuildBundle { build_output, artifact_plan, wasm_package_kind };
    validate_artifact_surface(&bundle, wasm_package_kind)?;
    Ok(bundle)
}

/// 从源码闭包直接生产完整目标产物报告。
pub fn compile_source_groups_to_artifacts(
    compiler: &ValkyrieCompiler,
    groups: &[CompilerSourceGroup],
    arch: &str,
    target: CanonicalTarget,
    clr_suspend_strategy: ClrSuspendStrategy,
    wasm_package_kind: emitter::nyar_backend_wasi::WasmPackageKind,
    output_dir: &Path,
    project_name: &str,
    emit_msil_sidecar: bool,
    emit_wat_sidecar: bool,
    generate_runtime_config: bool,
) -> Result<emitter::DriverCompileReport> {
    let bundle = compile_source_groups_to_backend_bundle(
        compiler,
        groups,
        arch,
        target,
        clr_suspend_strategy,
        wasm_package_kind,
    )?;
    emitter::compile_frontend_bundle_with_bundled_backends(
        &bundle,
        output_dir,
        project_name,
        emit_msil_sidecar,
        emit_wat_sidecar,
        generate_runtime_config,
    )
}

fn validate_artifact_surface(bundle: &CompilerBuildBundle, wasm_package_kind: emitter::nyar_backend_wasi::WasmPackageKind) -> Result<()> {
    if bundle.artifact_plan.target.to_profile(None).backend_family != TargetBackendFamily::Wasm {
        return Ok(());
    }
    let (export_count, entry_count) = bundle.surface_counts();
    match wasm_package_kind {
        emitter::nyar_backend_wasi::WasmPackageKind::Library if export_count == 0 => {
            Err(miette!("`artifact: library` requires at least one resolved export"))
        }
        emitter::nyar_backend_wasi::WasmPackageKind::Binary if entry_count == 0 => {
            Err(miette!("`artifact: binary` requires a resolved main entry"))
        }
        _ => Ok(()),
    }
}

impl FrontendBuildBundle for CompilerBuildBundle {
    fn planned_partitions(&self) -> &dyn PlannedArtifactPartitionsView {
        self
    }

    fn wasm_package_kind(&self) -> emitter::nyar_backend_wasi::WasmPackageKind {
        self.wasm_package_kind
    }

    fn submit_backend_input_for_partition(
        &self,
        partition_index: usize,
        backend_family: TargetBackendFamily,
        host_boundary: HostProjectionBoundary,
        output_dir: &Path,
        _lane: TargetLane,
    ) -> Result<LoweredBackendInput> {
        let fragment = assemble_fragment(&self.build_output, &self.artifact_plan, partition_index)?;
        let partition = self
            .artifact_plan
            .partitions
            .get(partition_index)
            .ok_or_else(|| miette!("分区索引 `{partition_index}` 超出范围"))?;
        let host_flavor = self.artifact_plan.target.to_profile(None).host_flavor;
        LoweredBackendInput::from_assembled_fragment(
            fragment,
            backend_family,
            host_boundary,
            output_dir,
            partition.lane,
            partition.clr_suspend_strategy,
            VmSuspendStrategy::default(),
            &host_flavor,
            self.wasm_package_kind,
        )
    }
}

impl PlannedArtifactPartitionsView for CompilerBuildBundle {
    fn primary_partition_name(&self) -> Option<String> {
        self.artifact_plan
            .partitions
            .iter()
            .find(|partition| partition.entry_operation.is_some())
            .map(|partition| partition.name.clone())
            .or_else(|| {
                self.artifact_plan
                    .partitions
                    .iter()
                    .find(|partition| partition.name.ends_with("::functions"))
                    .map(|partition| partition.name.clone())
            })
            .or_else(|| self.artifact_plan.partitions.first().map(|partition| partition.name.clone()))
    }

    fn partition_count(&self) -> usize {
        self.artifact_plan.partitions.len()
    }

    fn partition(&self, partition_index: usize) -> Option<&nyar::ArtifactPartition> {
        self.artifact_plan.partitions.get(partition_index)
    }

    fn backend_requirement(&self, partition_index: usize) -> Option<nyar::PartitionBackendRequirement> {
        self.artifact_plan.backend_requirement(partition_index)
    }
}

fn preprocess_target_templates(source: &str, arch: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut pos = 0;
    while pos < source.len() {
        let Some(rel) = source[pos..].find("<% match ") else {
            result.push_str(&source[pos..]);
            break;
        };
        let abs = pos + rel;
        result.push_str(&source[pos..abs]);
        match std_data::text::valkyrie::tgrammar::parse_tgrammar_fragment(&source[abs..]) {
            Ok((nodes, consumed)) if nodes.len() == 1 => {
                let fragment = &source[abs..abs + consumed];
                if let std_data::text::valkyrie::tgrammar::TgNode::Match(match_node) = &nodes[0]
                    && match_node.scrutinee.trim() == "arch"
                {
                    let selected = select_arch_match_body(match_node, arch);
                    result.push_str(&preprocess_target_templates(&tg_root_to_source(selected, fragment), arch));
                }
                else {
                    result.push_str(fragment);
                }
                pos = abs + consumed;
            }
            _ => {
                result.push_str(&source[abs..abs + 2]);
                pos = abs + 2;
            }
        }
    }
    result
}

fn select_arch_match_body<'a>(
    match_node: &'a std_data::text::valkyrie::tgrammar::TgMatch,
    arch: &str,
) -> &'a [std_data::text::valkyrie::tgrammar::TgNode] {
    for arm in &match_node.arms {
        if arm.pattern.as_deref().map(normalize_case_pattern).as_deref() == Some(arch) {
            return &arm.body;
        }
    }
    match_node
        .arms
        .iter()
        .find(|arm| arm.pattern.is_none())
        .map(|arm| arm.body.as_slice())
        .unwrap_or(&[])
}

fn normalize_case_pattern(pattern: &str) -> String {
    let pattern = pattern.trim();
    if (pattern.starts_with('"') && pattern.ends_with('"')) || (pattern.starts_with('\'') && pattern.ends_with('\'')) {
        pattern[1..pattern.len().saturating_sub(1)].to_string()
    }
    else {
        pattern.to_string()
    }
}

fn tg_root_to_source(nodes: &[std_data::text::valkyrie::tgrammar::TgNode], fragment: &str) -> String {
    nodes.iter().map(|node| node_to_source(node, fragment)).collect()
}

fn node_to_source(node: &std_data::text::valkyrie::tgrammar::TgNode, fragment: &str) -> String {
    use std_data::text::valkyrie::tgrammar::{TgIf, TgLoop, TgMatch, TgNode};
    let span = match node {
        TgNode::Text { span, .. } | TgNode::Stmt { span, .. } | TgNode::Comment { span, .. } => span.clone(),
        TgNode::If(TgIf { span, .. }) | TgNode::Loop(TgLoop { span, .. }) | TgNode::Match(TgMatch { span, .. }) => span.clone(),
    };
    fragment[span].to_string()
}
