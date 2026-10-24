//! CI：canonical Node bootstrap 夹具 → wasm 产物，供 `scripts/build.mjs capability` 装配。
//!
//! 使用仓库内 `tests/fixtures/bootstrap_node/entry_contract_canonical`（与
//! `bootstrap_node_entry` 同源），不依赖 `valkyrie.v` 的 `legion.tools` 自举完成度。
//! 不依赖 native `legion` 二进制；直接调用库内 `legion build` 实现。

use legion::cmds::build::{BuildArgs, run};
use rusty_valkyrie::CanonicalTarget;
use std::path::{Path, PathBuf};

fn bootstrap_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bootstrap_node/entry_contract_canonical")
}

fn capability_out_root() -> PathBuf {
    std::env::var("LEGION_CAPABILITY_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..").join("dist/legion-node-capability"))
}

fn artifact_dir(out_root: &Path) -> PathBuf {
    let nested = out_root.join("wasm32-node-unknown-wasm");
    if nested.join("legion.wasm").is_file() {
        return nested;
    }
    out_root.to_path_buf()
}

#[test]
fn assemble_vcc_unknown_wasm32_capability() {
    let fixture = bootstrap_fixture_root();
    assert!(fixture.join("legion.von").is_file(), "missing bootstrap fixture at {}", fixture.display());

    let out_root = capability_out_root();
    std::fs::create_dir_all(&out_root).expect("create capability output dir");

    let status = run(&BuildArgs {
        project_dir: fixture.clone(),
        target: CanonicalTarget::parse("node").expect("node target"),
        output_dir: Some(out_root.clone()),
        workspace: false,
        debug_artifacts: false,
    })
    .expect("legion build bootstrap node fixture");

    assert_eq!(status, std::process::ExitCode::SUCCESS);

    let artifacts = artifact_dir(&out_root);
    assert!(artifacts.join("legion.wasm").is_file(), "missing legion.wasm under {}", artifacts.display());
    assert!(artifacts.join("run-contracts.txt").is_file(), "missing run-contracts.txt under {}", artifacts.display());
}
