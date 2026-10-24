//! 打印语义源码快照各 group 的体积与 parse 结果；用于定位 parser 失败偏移。
//!
//! ```text
//! cargo run -p legion --example dump_group_offset -- /path/to/legion-project
//! ```
use std::path::PathBuf;

use legion_workspace::{
    planner::{BuildRequest, WorkspaceResolver},
    source_snapshot::compile_source_snapshot,
};

fn project_dir() -> PathBuf {
    if let Ok(path) = std::env::var("LEGION_PROJECT_DIR") {
        return PathBuf::from(path);
    }
    std::env::args().nth(1).map(PathBuf::from).expect("usage: dump_group_offset <legion-project-dir> (or set LEGION_PROJECT_DIR)")
}

fn main() {
    let project = project_dir();
    let workspace = WorkspaceResolver::discover_for_project(&project).expect("workspace");
    let plan = workspace
        .build_plan(&BuildRequest { project_dir: project, target: legion::CanonicalTarget::parse("node").expect("node"), output_dir: None })
        .expect("plan");
    let groups = compile_source_snapshot(&plan.project.semantic_source_groups).expect("snapshot");
    let probe_offset = std::env::args().nth(2).and_then(|value| value.parse().ok()).unwrap_or(782usize);
    for (index, group) in groups.iter().enumerate() {
        eprintln!("group {} key={} files={}", index, group.dependency_key, group.sources.len());
        for file in &group.sources {
            if file.path.ends_with("collection/Array.v") {
                eprintln!("  array source: {} bytes={}", file.path, file.source.len());
            }
            if file.source.len() > probe_offset {
                let start = file.source.floor_char_boundary(probe_offset.saturating_sub(22));
                let end = file.source.ceil_char_boundary((probe_offset + 28).min(file.source.len()));
                let probe_start = file.source.floor_char_boundary(probe_offset);
                let probe_end = file.source.ceil_char_boundary((probe_offset + 18).min(file.source.len()));
                eprintln!("  {} @{}: `{}`", file.path, probe_offset, &file.source[probe_start..probe_end]);
                eprintln!("  snippet:\n{}\n", &file.source[start..end]);
            }
            match rusty_valkyrie::frontend::parse_source(&file.source) {
                Ok(root) => eprintln!("  {} parse ok items={}", file.path, root.items.len()),
                Err(error) => eprintln!("  {} parse err: {:?}", file.path, error),
            }
        }
    }
}
