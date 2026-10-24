//! 将 Resolver 的源码组交给 Compiler；依赖身份不因组数或缓存状态改变。

use std::fs;

use miette::{IntoDiagnostic, Result, miette};
use nyar_types::compiler_input::{CompilerSourceFile, CompilerSourceGroup};

use crate::planner::PlannedSemanticSourceGroup;

/// 单个语义源码组允许纳入的最大字节数。
const MAX_GROUP_SOURCE_BYTES: usize = 16 * 1024 * 1024;
/// 全部语义组拼接后的总字节数上限。
const MAX_TOTAL_SOURCE_BYTES: usize = 64 * 1024 * 1024;

/// 读取并校验 `groups` 中的源码文件，组装为 Compiler 可消费的源码组快照。
pub fn compile_source_snapshot(groups: &[PlannedSemanticSourceGroup]) -> Result<Vec<CompilerSourceGroup>> {
    if groups.is_empty() {
        return Err(miette!("semantic source group plan is empty"));
    }

    let mut staged_sources = Vec::with_capacity(groups.len());
    let mut total_bytes = 0usize;
    for group in groups {
        let manifest_dir = fs::canonicalize(&group.manifest_dir).into_diagnostic()?;
        let mut sources = Vec::with_capacity(group.source_files.len());
        let mut group_bytes = 0usize;
        for path in &group.source_files {
            let resolved_path = fs::canonicalize(path)
                .into_diagnostic()
                .map_err(|error| error.wrap_err(format!("读取源码失败：源码组 `{}`", group.dependency_key)))?;
            let logical_path = resolved_path
                .strip_prefix(&manifest_dir)
                .map_err(|_| miette!("源码文件不属于源码组 `{}` 的 manifest 目录", group.dependency_key))?
                .to_string_lossy()
                .replace('\\', "/");
            let content = fs::read_to_string(path)
                .into_diagnostic()
                .map_err(|error| error.wrap_err(format!("读取源码失败 {}::{logical_path}", group.dependency_key)))?;
            let normalized = content.strip_prefix('\u{FEFF}').unwrap_or(&content);
            group_bytes += normalized.len();
            if group_bytes > MAX_GROUP_SOURCE_BYTES {
                return Err(miette!(
                    "semantic source group `{}` exceeds {} byte limit (would be {} bytes)",
                    group.dependency_key,
                    MAX_GROUP_SOURCE_BYTES,
                    group_bytes
                ));
            }
            total_bytes += normalized.len();
            if total_bytes > MAX_TOTAL_SOURCE_BYTES {
                return Err(miette!("semantic source snapshot exceeds {} byte total limit", MAX_TOTAL_SOURCE_BYTES));
            }
            sources.push(CompilerSourceFile { path: logical_path, source: normalized.to_owned() });
        }
        staged_sources.push((group, sources));
    }

    // 装配层只验证存在显式入口组并读取文件；入口数量、依赖拓扑与语义索引属于 Compiler。
    // 独立生成源码组会在 Asgard 装配层作为显式入口追加，不能被这里限制成「唯一入口且位于最后」。
    if !groups.iter().any(|group| group.is_entry) {
        return Err(miette!("源码快照缺少显式入口组"));
    }

    let mut compiler_groups = Vec::with_capacity(groups.len());
    for (group, sources) in staged_sources {
        compiler_groups.push(CompilerSourceGroup {
            dependency_key: group.dependency_key.clone(),
            name: group.name.clone(),
            sources,
            direct_dependencies: group.direct_dependencies.clone(),
            is_entry: group.is_entry,
        });
    }
    Ok(compiler_groups)
}
