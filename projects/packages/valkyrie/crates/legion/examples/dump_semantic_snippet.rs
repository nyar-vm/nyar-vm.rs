//! 打印语义快照中指定 group 在某一字节偏移附近的片段。
//!
//! ```text
//! cargo run -p legion --example dump_semantic_snippet -- /path/to/project 0 23581
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
    std::env::args().nth(1).map(PathBuf::from).expect("usage: dump_semantic_snippet <legion-project-dir> [group-index] [offset]")
}

fn main() {
    let project = project_dir();
    let group_index = std::env::args().nth(2).and_then(|value| value.parse().ok()).unwrap_or(0usize);
    let offset = std::env::args().nth(3).and_then(|value| value.parse().ok()).unwrap_or(0usize);
    let workspace = WorkspaceResolver::discover_for_project(&project).expect("workspace");
    let plan = workspace
        .build_plan(&BuildRequest { project_dir: project, target: legion::CanonicalTarget::parse("node").expect("node"), output_dir: None })
        .expect("plan");
    let groups = compile_source_snapshot(&plan.project.semantic_source_groups).expect("snapshot");
    let Some(group) = groups.get(group_index)
    else {
        panic!("group index {} out of range (len={})", group_index, groups.len());
    };
    // 源码组由多个文件组成；调试片段按文件顺序拼接，偏移相对拼接结果。
    let source = group.sources.iter().map(|file| file.source.as_str()).collect::<Vec<_>>().join("\n");
    eprintln!("group {} key={} len={}", group_index, group.dependency_key, source.len());
    if offset >= source.len() {
        panic!("offset {} out of range for group len {}", offset, source.len());
    }
    let end = (offset + 24).min(source.len());
    eprintln!("span {}..{} = `{}`", offset, end, &source[offset..end]);
    for probe in [offset.saturating_sub(11), offset, offset + 11, offset + 22] {
        if probe >= source.len() {
            continue;
        }
        let start = source.floor_char_boundary(probe.saturating_sub(40));
        let end = source.ceil_char_boundary((probe + 40).min(source.len()));
        eprintln!("--- @{}:\n{}\n", probe, &source[start..end]);
    }
    match rusty_valkyrie::frontend::parse_source(&source) {
        Ok(root) => eprintln!("parse ok items={}", root.items.len()),
        Err(error) => eprintln!("parse err: {:?}", error),
    }
}
