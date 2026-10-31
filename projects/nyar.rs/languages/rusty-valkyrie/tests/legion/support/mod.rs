//! Inline smoke project helpers for Legion integration tests (no legion-fixtures corpus).

use std::{
    fs,
    path::{Path, PathBuf},
};

use tempfile::TempDir;

pub struct SmokeProject {
    _temp_dir: TempDir,
    pub project_dir: PathBuf,
}

/// 源级 `core/primitive` 声明族：数值原始类型名不是语言内建，必须绑定到源码声明身份。
/// 夹具项目声明 `"core": false`，因此在 `source/` 下写入等价声明文件；属性值给出
/// `core::primitive::*` 身份，与内联编译测试的 `PRIMITIVE_I32` 保持同一合同。
const CORE_PRIMITIVE_DECLARATIONS: &str = concat!(
    "[primitive(\"core::primitive::i8\")] structure i8 { }\n",
    "[primitive(\"core::primitive::i16\")] structure i16 { }\n",
    "[primitive(\"core::primitive::i32\")] structure i32 { }\n",
    "[primitive(\"core::primitive::i64\")] structure i64 { }\n",
    "[primitive(\"core::primitive::i128\")] structure i128 { }\n",
    "[primitive(\"core::primitive::u8\")] structure u8 { }\n",
    "[primitive(\"core::primitive::u16\")] structure u16 { }\n",
    "[primitive(\"core::primitive::u32\")] structure u32 { }\n",
    "[primitive(\"core::primitive::u64\")] structure u64 { }\n",
    "[primitive(\"core::primitive::u128\")] structure u128 { }\n",
    "[primitive(\"core::primitive::isize\")] structure isize { }\n",
    "[primitive(\"core::primitive::usize\")] structure usize { }\n",
    "[primitive(\"core::primitive::f32\")] structure f32 { }\n",
    "[primitive(\"core::primitive::f64\")] structure f64 { }\n",
    "[primitive(\"core::primitive::f128\")] structure f128 { }\n",
    "[primitive(\"core::primitive::byte\")] structure byte { }\n",
    "[primitive(\"core::primitive::sbyte\")] structure sbyte { }\n",
    // 运行时夹具的 `main` 以 `ExitCode` 表达进程退出码；该名字来自 `std` 适配层，
    // 夹具项目声明 `\"std\": false`，因此在源级给出等价别名，避免依赖未链接的 std 组。
    "type ExitCode = i32;\n",
    "micro ExitCode(value: i32): i32 { return value }\n",
);

/// 写入夹具入口源码，并同时提供源级 primitive 声明。
fn write_main_source(source_dir: &Path, source: &str) {
    fs::write(source_dir.join("main.v"), source).unwrap();
    write_core_primitives(source_dir);
}

fn write_core_primitives(source_dir: &Path) {
    fs::write(source_dir.join("core_primitive.v"), CORE_PRIMITIVE_DECLARATIONS).unwrap();
}

/// Workspace：`app` + `std` + `std.adaptor.wasm`，入口经 `[main]` 调用 `std.math::random_next_i32`（browser wasm + JS glue）。
pub fn create_browser_wasm_host_random_workspace(prefix: &str) -> SmokeProject {
    let temp_dir = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app");
    let wasm_adaptor_dir = root.join("std.adaptor.wasm");
    let std_dir = root.join("std");
    let core_dir = root.join("core");

    fs::create_dir_all(app_dir.join("source")).unwrap();
    fs::create_dir_all(wasm_adaptor_dir.join("source")).unwrap();
    fs::create_dir_all(std_dir.join("source")).unwrap();
    fs::create_dir_all(core_dir.join("source")).unwrap();

    fs::write(
        root.join("legions.von"),
        r#"{
    name: "browser-wasm-host-random",
    members: ["app", "std.adaptor.wasm", "std", "core"]
}
"#,
    )
    .unwrap();
    fs::write(
        core_dir.join("legion.von"),
        r#"{
    name: "core",
    build: [{ target: "wasm32-unknown-browser-wasm" }]
}
"#,
    )
    .unwrap();
    write_core_primitives(&core_dir.join("source"));
    fs::write(
        std_dir.join("legion.von"),
        r#"{
    name: "std",
    dependencies: { "core": "workspace" },
    build: [{ target: "wasm32-unknown-browser-wasm" }]
}
"#,
    )
    .unwrap();
    fs::write(
        std_dir.join("source").join("random.v"),
        r#"namespace std.math;
[host_contract]
micro random_next_i32(seed: i32): i32
"#,
    )
    .unwrap();

    fs::write(
        wasm_adaptor_dir.join("legion.von"),
        r#"{
    name: "std.adaptor.wasm",
    dependencies: { "std": "workspace", "core": "workspace" },
    build: [{ target: "wasm32-unknown-browser-wasm" }]
}
"#,
    )
    .unwrap();
    fs::create_dir_all(wasm_adaptor_dir.join("source").join("math").join("random")).unwrap();
    fs::write(
        wasm_adaptor_dir.join("source").join("math").join("random").join("Random.browser.v"),
        r#"[when(target.specification == "browser")]
namespace std.adaptor.wasm;

[host_provider(std::math::random_next_i32)]
[js("random")]
micro js_random_next_i32(seed: i32): i32
"#,
    )
    .unwrap();

    fs::write(
        app_dir.join("legion.von"),
        r#"{
    name: "app",
    dependencies: {
        "std": "workspace",
        "std.adaptor.wasm": "workspace",
        "core": "workspace"
    },
    build: [{ target: "wasm32-unknown-browser-wasm", publish: ["web-app"] }]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("source").join("main.v"),
        r#"namespace std.math;
[main]
micro main() -> i32 {
    return random_next_i32(1)
}
"#,
    )
    .unwrap();

    SmokeProject { _temp_dir: temp_dir, project_dir: canonicalize_lossy(&app_dir) }
}

/// Workspace：`app` + `std` + `std.adaptor.wasi`，WASI 目标下由 WASI adaptor 绑定 random 合同（无 `env` guest import）。
pub fn create_wasi_host_random_workspace(prefix: &str) -> SmokeProject {
    let temp_dir = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app");
    let wasi_dir = root.join("std.adaptor.wasi");
    let std_dir = root.join("std");
    let core_dir = root.join("core");

    fs::create_dir_all(app_dir.join("source")).unwrap();
    fs::create_dir_all(wasi_dir.join("source").join("math").join("random")).unwrap();
    fs::create_dir_all(std_dir.join("source")).unwrap();
    fs::create_dir_all(core_dir.join("source")).unwrap();

    fs::write(
        root.join("legions.von"),
        r#"{
    name: "wasi-host-random",
    members: ["app", "std.adaptor.wasi", "std", "core"]
}
"#,
    )
    .unwrap();
    fs::write(
        core_dir.join("legion.von"),
        r#"{
    name: "core",
    build: [{ target: "wasi" }]
}
"#,
    )
    .unwrap();
    write_core_primitives(&core_dir.join("source"));
    fs::write(
        std_dir.join("legion.von"),
        r#"{
    name: "std",
    dependencies: { "core": "workspace" },
    build: [{ target: "wasi" }]
}
"#,
    )
    .unwrap();
    fs::write(
        std_dir.join("source").join("random.v"),
        r#"namespace std.math;
[host_contract]
micro random_next_i32(seed: i32): i32
"#,
    )
    .unwrap();
    fs::write(
        wasi_dir.join("legion.von"),
        r#"{
    name: "std.adaptor.wasi",
    dependencies: { "std": "workspace", "core": "workspace" },
    sdk_vendor: {
        organization: "valkyrie",
        host: "wasi",
        kind: "distribution-default",
        targets: ["wasi", "wasip3"]
    },
    build: [{ target: "wasi" }]
}
"#,
    )
    .unwrap();
    fs::write(
        wasi_dir.join("source").join("math").join("random").join("Random.wasi.v"),
        r#"[when(target.specification == "wasi")]
namespace std.adaptor.wasi;

[host_provider(std::math::random_next_i32)]
micro wasi_random_next_i32(seed: i32): i32 {
    return 0
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("legion.von"),
        r#"{
    name: "app",
    dependencies: {
        "std": "workspace",
        "std.adaptor.wasi": "workspace",
        "core": "workspace"
    },
    build: [{ target: "wasi" }]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("source").join("main.v"),
        r#"namespace std.math;
[main]
micro main() -> i32 {
    return random_next_i32(1)
}
"#,
    )
    .unwrap();

    SmokeProject { _temp_dir: temp_dir, project_dir: canonicalize_lossy(&app_dir) }
}

/// `std` 自带 `[host_contract]` 函数体作 Valkyrie fallback；不依赖任何 adaptor。
pub fn create_valkyrie_fallback_random_workspace(prefix: &str) -> SmokeProject {
    let temp_dir = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app");
    let std_dir = root.join("std");
    let core_dir = root.join("core");

    fs::create_dir_all(app_dir.join("source")).unwrap();
    fs::create_dir_all(std_dir.join("source")).unwrap();
    fs::create_dir_all(core_dir.join("source")).unwrap();

    fs::write(
        root.join("legions.von"),
        r#"{
    name: "valkyrie-fallback-random",
    members: ["app", "std", "core"]
}
"#,
    )
    .unwrap();
    fs::write(
        core_dir.join("legion.von"),
        r#"{
    name: "core",
    build: [{ target: "wasm32-unknown-browser-wasm" }]
}
"#,
    )
    .unwrap();
    write_core_primitives(&core_dir.join("source"));
    fs::write(
        std_dir.join("legion.von"),
        r#"{
    name: "std",
    dependencies: { "core": "workspace" },
    build: [{ target: "wasm32-unknown-browser-wasm" }]
}
"#,
    )
    .unwrap();
    fs::write(
        std_dir.join("source").join("random.v"),
        r#"namespace std.math;
[host_contract]
micro random_next_i32(seed: i32): i32 {
    return seed
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("legion.von"),
        r#"{
    name: "app",
    dependencies: {
        "std": "workspace",
        "core": "workspace"
    },
    build: [{ target: "wasm32-unknown-browser-wasm" }]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("source").join("main.v"),
        r#"namespace std.math;
[main]
micro main() -> i32 {
    return random_next_i32(1)
}
"#,
    )
    .unwrap();

    SmokeProject { _temp_dir: temp_dir, project_dir: canonicalize_lossy(&app_dir) }
}

/// 与 [`create_browser_wasm_host_random_workspace`] 相同，但 `std` 有未引用的 `fetch_i32_stub` 合同，adaptor **`providers.v` 同文件** 含 fetch+random 两个 bodyless `[js]`；
/// 入口只调用 random（planner + `prune_unselected_host_provider_external_calls`）。
pub fn create_browser_wasm_dual_host_prune_workspace(prefix: &str) -> SmokeProject {
    let fixture = create_browser_wasm_host_random_workspace(prefix);
    let root = fixture.project_dir.parent().expect("workspace root");
    let std_dir = root.join("std");
    let wasm_adaptor_dir = root.join("std.adaptor.wasm");

    fs::write(
        std_dir.join("source").join("z_fetch_stub.v"),
        r#"namespace std.hooks;
[host_contract]
micro fetch_i32_stub(path: i32): i32
"#,
    )
    .unwrap();
    fs::write(
        wasm_adaptor_dir.join("source").join("providers.v"),
        r#"namespace std.adaptor.wasm;

[host_provider(std::hooks::fetch_i32_stub)]
[js("fetch")]
micro js_fetch_i32_stub(path: i32): i32

[host_provider(std::math::random_next_i32)]
[js("random")]
micro js_random_next_i32(seed: i32): i32
"#,
    )
    .unwrap();
    let _ = fs::remove_file(wasm_adaptor_dir.join("source").join("math").join("random").join("Random.browser.v"));
    fixture
}

pub fn create_smoke_project(prefix: &str) -> SmokeProject {
    create_fixture(
        prefix,
        default_build_manifest(),
        r#"[main]
micro main(): i64 {
    return 0
}
"#,
    )
}

pub fn create_smoke_project_with_source(prefix: &str, source: &str) -> SmokeProject {
    create_fixture(prefix, default_build_manifest(), source)
}

pub fn create_smoke_project_with_build(prefix: &str, build_block: &str, source: &str) -> SmokeProject {
    create_fixture(prefix, build_block, source)
}

pub fn create_smoke_project_with_manifest(prefix: &str, manifest: &str, source: &str) -> SmokeProject {
    create_manifest_fixture(prefix, manifest, source, true)
}

pub fn create_local_package_project_with_manifest(prefix: &str, manifest: &str, source: &str) -> SmokeProject {
    create_manifest_fixture(prefix, manifest, source, false)
}

pub fn create_script_project_with_manifest(prefix: &str, manifest: &str, source: &str) -> SmokeProject {
    create_standalone_manifest_fixture(prefix, manifest, source)
}

pub fn create_nested_workspace_member_project_with_manifest(prefix: &str, manifest: &str, source: &str) -> SmokeProject {
    create_nested_workspace_fixture(prefix, Path::new("subspace").join("projects").join("app"), manifest, source)
}

pub fn create_local_package_project(prefix: &str, build_block: &str, source: &str) -> SmokeProject {
    let temp_dir = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
    let root = temp_dir.path();
    let project_dir = root.join("app");
    let source_dir = project_dir.join("source");
    fs::create_dir_all(&source_dir).unwrap();
    fs::write(
        root.join("legions.von"),
        r#"{
    name: "runtime-smoke",
    members: []
}
"#,
    )
    .unwrap();
    fs::write(
        project_dir.join("legion.von"),
        format!(
            r#"{{
    name: "app",
    build: [
        {}
    ]
}}
"#,
            build_block
        ),
    )
    .unwrap();
    write_main_source(&source_dir, source);

    SmokeProject { _temp_dir: temp_dir, project_dir: canonicalize_lossy(&project_dir) }
}

fn create_fixture(prefix: &str, build_block: &str, source: &str) -> SmokeProject {
    let manifest = format!(
        r#"{{
    name: "app",
    build: [
        {}
    ]
}}
"#,
        build_block
    );
    create_manifest_fixture(prefix, &manifest, source, true)
}

fn create_manifest_fixture(prefix: &str, manifest: &str, source: &str, include_workspace_member: bool) -> SmokeProject {
    create_nested_manifest_fixture(prefix, PathBuf::from("app"), manifest, source, include_workspace_member)
}

fn create_nested_manifest_fixture(
    prefix: &str,
    project_relative_dir: PathBuf,
    manifest: &str,
    source: &str,
    include_workspace_member: bool,
) -> SmokeProject {
    let temp_dir = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
    let root = temp_dir.path();
    let project_dir = root.join(&project_relative_dir);
    let source_dir = project_dir.join("source");
    fs::create_dir_all(&source_dir).unwrap();
    let workspace_member = project_relative_dir.to_string_lossy().replace('\\', "/");
    let workspace_manifest = if include_workspace_member {
        format!(
            r#"{{
    name: "runtime-smoke",
    members: [
        "{}"
    ]
}}
"#,
            workspace_member
        )
    }
    else {
        r#"{
    name: "runtime-smoke",
    members: []
}
"#
        .to_string()
    };
    fs::write(root.join("legions.von"), workspace_manifest).unwrap();
    fs::write(project_dir.join("legion.von"), manifest).unwrap();
    write_main_source(&source_dir, source);

    SmokeProject { _temp_dir: temp_dir, project_dir: canonicalize_lossy(&project_dir) }
}

fn create_nested_workspace_fixture(prefix: &str, project_relative_dir: PathBuf, manifest: &str, source: &str) -> SmokeProject {
    let temp_dir = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
    let root = temp_dir.path();
    let subspace_dir = root.join("subspace");
    let project_dir = root.join(&project_relative_dir);
    let source_dir = project_dir.join("source");
    let nested_member = project_relative_dir
        .strip_prefix("subspace")
        .unwrap_or(project_relative_dir.as_path())
        .to_string_lossy()
        .trim_start_matches(['\\', '/'])
        .replace('\\', "/");
    fs::create_dir_all(&source_dir).unwrap();
    fs::write(
        root.join("legions.von"),
        r#"{
    name: "runtime-smoke",
    members: [
        "subspace"
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        subspace_dir.join("legions.von"),
        format!(
            r#"{{
    name: "nested-runtime-smoke",
    members: [
        "{nested_member}"
    ]
}}
"#
        ),
    )
    .unwrap();
    fs::write(project_dir.join("legion.von"), manifest).unwrap();
    write_main_source(&source_dir, source);

    SmokeProject { _temp_dir: temp_dir, project_dir: canonicalize_lossy(&project_dir) }
}

fn create_root_manifest_fixture(prefix: &str, manifest: &str, source: &str, include_workspace_member: bool) -> SmokeProject {
    let temp_dir = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
    let root = temp_dir.path().to_path_buf();
    let source_dir = root.join("source");
    fs::create_dir_all(&source_dir).unwrap();
    let workspace_manifest = if include_workspace_member {
        r#"{
    name: "runtime-smoke",
    members: [
        "."
    ]
}
"#
    }
    else {
        r#"{
    name: "runtime-smoke",
    members: []
}
"#
    };
    fs::write(root.join("legions.von"), workspace_manifest).unwrap();
    fs::write(root.join("legion.von"), manifest).unwrap();
    write_main_source(&source_dir, source);

    SmokeProject { _temp_dir: temp_dir, project_dir: canonicalize_lossy(&root) }
}

fn create_standalone_manifest_fixture(prefix: &str, manifest: &str, source: &str) -> SmokeProject {
    let temp_dir = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
    let root = temp_dir.path().to_path_buf();
    let source_dir = root.join("source");
    fs::create_dir_all(&source_dir).unwrap();
    fs::write(root.join("legion.von"), manifest).unwrap();
    write_main_source(&source_dir, source);

    SmokeProject { _temp_dir: temp_dir, project_dir: canonicalize_lossy(&root) }
}

fn default_build_manifest() -> &'static str {
    r#"{
            target: "clr",
            msil: true
        }"#
}

/// 规范化路径（失败时保留原路径）。
fn canonicalize_lossy(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
