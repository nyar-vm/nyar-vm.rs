//! P0.5：冻结仍可达的 vcc-data 生产消费者清单。
//!
//! 正式链默认不链接 vcc-data（见 `production_lane_contract`）。
//! 本测试登记 Asgard / Legion spy 中仍导入该 crate 的源文件；
//! 新增消费者必须先迁移到 Acorn 或正式格式所有者，不得静默扩大清单。

use std::{
    fs,
    path::{Path, PathBuf},
};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..").canonicalize().expect("workspace root")
}

/// 相对 `valkyrie.rs` 根的已冻结消费者路径（正斜杠）。
///
/// 已迁移至 Acorn 并从本清单移除：`legion spy wasm`（`acorn-wasm`）、
/// `legion spy jvm`（`acorn-jvm`）、`legion spy clr` / `pe_dump`（`acorn-pe`）、
/// `legion spy native`（`acorn-x64`）、Asgard 的 `dex` / `mach_o` / `host_exe` /
/// `aarch64` / `elf` 消费者（`acorn-dex` / `acorn-mach-o` / `acorn-host` /
/// `acorn-aarch64` / `acorn-pe`）及其测试。
/// 清单已清空：`vcc-data` 不再有生产消费者，任何新消费者都必须先停止。
const FROZEN_VCC_DATA_CONSUMERS: &[&str] = &[];

fn collect_vcc_data_consumers(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let scan_roots = [root.join("projects/compilers/asgard"), root.join("projects/compilers/legion")];
    for scan_root in scan_roots {
        walk_rs(&scan_root, root, &mut found);
    }
    found.sort();
    found.dedup();
    found
}

fn walk_rs(dir: &Path, workspace: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir)
    else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name == "target" || name == "node_modules" {
                continue;
            }
            walk_rs(&path, workspace, out);
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        if path.file_name().and_then(|s| s.to_str()) == Some("vcc_data_consumer_inventory.rs") {
            continue;
        }
        let Ok(source) = fs::read_to_string(&path)
        else {
            continue;
        };
        if !source.contains("vcc_data::") && !source.contains("use vcc_data") {
            continue;
        }
        let rel = path.strip_prefix(workspace).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        out.push(rel);
    }
}

#[test]
fn vcc_data_consumers_match_frozen_inventory() {
    let root = workspace_root();
    let found = collect_vcc_data_consumers(&root);
    let expected: Vec<String> = FROZEN_VCC_DATA_CONSUMERS.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(
        found, expected,
        "vcc-data 消费者清单变化。\n\
         新增路径必须先迁移到 Acorn/正式格式所有者再删除本清单项；\n\
         不得扩大旁支可达面。\n\
         found={found:?}\n\
         expected={expected:?}"
    );
}
