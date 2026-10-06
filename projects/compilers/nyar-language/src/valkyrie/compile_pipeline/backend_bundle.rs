//! Compiler 生成的完整目标编译 bundle。
//!
//! Legion 与其他装配层只能提交源码快照和目标合同；分区规划、fragment
//! assembly 以及目标私有输入都在 Compiler 内完成。

use std::path::Path;

use nyar_emitter::{FrontendBuildBundle, PlannedArtifactPartitionsView};
use miette::{Result, miette};
use nyar::{ArtifactPartitionPlan, CanonicalTarget, ClrSuspendStrategy, TargetBackendFamily};

use crate::{
    CompilerSourceGroup, ValkyrieCompiler,
    valkyrie::{assemble_fragment, build_output_surface_counts, plan_artifacts_from_compiled_program},
};

/// Compiler 已完成语义分析、表示规划和分区装配的目标输入 bundle。
struct CompilerBuildBundle {
    compiled_program: nyar_types::CompiledProgram,
    artifact_plan: ArtifactPartitionPlan,
    wasm_package_kind: nyar_emitter::nyar_backend_wasi::WasmPackageKind,
}

impl CompilerBuildBundle {
    /// 返回已由 Canonical 成功载荷确认的导出和入口数量。
    fn surface_counts(&self) -> (usize, usize) {
        build_output_surface_counts(&self.compiled_program)
    }
}

/// 从 Resolver 提供的源码组生产唯一目标 bundle。
fn compile_source_groups_to_backend_bundle(
    compiler: &ValkyrieCompiler,
    groups: &[CompilerSourceGroup],
    arch: &str,
    target: CanonicalTarget,
    clr_suspend_strategy: ClrSuspendStrategy,
    wasm_package_kind: nyar_emitter::nyar_backend_wasi::WasmPackageKind,
) -> Result<CompilerBuildBundle> {
    let groups = groups
        .iter()
        .cloned()
        .map(|mut group| {
            group.source = crate::transitional::tgrammar::preprocess_target_templates(&group.source, arch);
            group
        })
        .collect::<Vec<_>>();
    let compiled_program =
        compiler.compile_source_groups_to_program(&groups).map_err(|error| miette!("Compiler semantic snapshot failed: {error}"))?;
    let artifact_plan = plan_artifacts_from_compiled_program(&compiled_program, target, clr_suspend_strategy)
        .map_err(|error| miette!("Compiler representation planning failed: {error:?}"))?;
    let bundle = CompilerBuildBundle { compiled_program, artifact_plan, wasm_package_kind };
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
    wasm_package_kind: nyar_emitter::nyar_backend_wasi::WasmPackageKind,
    output_dir: &Path,
    project_name: &str,
    emit_wat_sidecar: bool,
    generate_runtime_config: bool,
) -> Result<nyar_emitter::DriverCompileReport> {
    let bundle = compile_source_groups_to_backend_bundle(compiler, groups, arch, target, clr_suspend_strategy, wasm_package_kind)?;
    nyar_emitter::compile_frontend_bundle_with_bundled_backends(&bundle, output_dir, project_name, emit_wat_sidecar, generate_runtime_config)
}

fn validate_artifact_surface(bundle: &CompilerBuildBundle, wasm_package_kind: nyar_emitter::nyar_backend_wasi::WasmPackageKind) -> Result<()> {
    if bundle.artifact_plan.target.to_profile(None).backend_family != TargetBackendFamily::Wasm {
        return Ok(());
    }
    let (export_count, entry_count) = bundle.surface_counts();
    match wasm_package_kind {
        nyar_emitter::nyar_backend_wasi::WasmPackageKind::Library if export_count == 0 => {
            Err(miette!("`artifact: library` requires at least one resolved export"))
        }
        nyar_emitter::nyar_backend_wasi::WasmPackageKind::Binary if entry_count == 0 => {
            Err(miette!("`artifact: binary` requires a resolved main entry"))
        }
        _ => Ok(()),
    }
}

impl FrontendBuildBundle for CompilerBuildBundle {
    fn planned_partitions(&self) -> &dyn PlannedArtifactPartitionsView {
        self
    }

    fn wasm_package_kind(&self) -> nyar_emitter::nyar_backend_wasi::WasmPackageKind {
        self.wasm_package_kind
    }

    fn assemble_fragment_for_partition(&self, partition_index: usize) -> Result<nyar::AssembledFragment> {
        assemble_fragment(&self.compiled_program, &self.artifact_plan, partition_index)
            .map_err(|error| miette!("Compiler fragment 装配失败: {error}"))
    }

    fn target_host_flavor(&self) -> Option<String> {
        Some(self.artifact_plan.target.to_profile(None).host_flavor)
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
