mod support;

use std::{
    fs,
    path::{Path, PathBuf},
};

use legion::CanonicalTarget;
use legion_workspace::planner::{BuildRequest, ProjectResolutionMode, WorkspaceResolver, canonical_target};
use miette::{GraphicalReportHandler, Report};
use support::{
    create_local_package_project_with_manifest, create_nested_workspace_member_project_with_manifest, create_smoke_project_with_build,
    create_smoke_project_with_manifest,
};
use tempfile::{Builder, TempDir};

struct WorkspaceFixture {
    _temp_dir: TempDir,
    root_dir: PathBuf,
    project_dir: PathBuf,
}

#[test]
fn canonicalizes_short_targets() {
    assert_eq!(canonical_target("clr").unwrap().to_string(), "clr-microsoft-unknown-managed");
    assert_eq!(canonical_target("jvm").unwrap().to_string(), "jvm-openjdk-unknown-managed");
    assert_eq!(canonical_target("wasm").unwrap().to_string(), "wasm32-unknown-browser-wasm");
    assert_eq!(canonical_target("node").unwrap().to_string(), "wasm32-node-unknown-wasm");
}

#[test]
fn node_default_output_dir_matches_publish_layout() {
    let fixture = create_smoke_project_with_build(
        "legion-planner-node-output",
        r#"{
            target: "node"
        }"#,
        r#"[main]
micro main() -> i64 {
    return 0;
}
"#,
    );
    let workspace = WorkspaceResolver::discover(&fixture.project_dir).unwrap();
    let plan = workspace
        .build_plan(&BuildRequest {
            project_dir: fixture.project_dir.clone(),
            target: CanonicalTarget::parse("node").unwrap(),
            output_dir: None,
        })
        .unwrap();
    assert_eq!(plan.output_dir, fixture.project_dir.join("dist").join("wasm32-node-unknown-wasm"));
}

#[test]
fn synthesizes_cli_build_target_for_library_without_manifest_entry() {
    let fixture = create_smoke_project_with_manifest(
        "legion-planner-library-nyar",
        r#"{
    name: "lib-app",
    artifact: "library",
    build: [
        { target: "node" }
    ]
}"#,
        r#"micro main() -> i64 {
    return 0;
}
"#,
    );
    let workspace = WorkspaceResolver::discover(&fixture.project_dir).unwrap();
    let nyar = CanonicalTarget::parse("ovm").unwrap();
    let plan = workspace.build_plan(&BuildRequest { project_dir: fixture.project_dir.clone(), target: nyar, output_dir: None }).unwrap();
    assert_eq!(plan.project.build_target.target, nyar);
}

#[test]
fn discovers_temp_workspace_build_plan() {
    let fixture = create_smoke_project_with_build(
        "legion-planner",
        r#"{
            target: "clr"
        }"#,
        r#"micro main() -> i64 {
    return 0;
}
"#,
    );
    let workspace = WorkspaceResolver::discover(&fixture.project_dir).unwrap();
    let plan = workspace
        .build_plan(&BuildRequest { project_dir: fixture.project_dir.clone(), target: CanonicalTarget::clr(), output_dir: None })
        .unwrap();

    assert_eq!(plan.project.name, "app");
    assert_eq!(plan.project.build_target.target, CanonicalTarget::clr());
    assert!(plan.project.source_files.iter().any(|path| path.ends_with(std::path::Path::new("source").join("main.v"))));
}

#[test]
fn renders_pretty_report_for_missing_workspace() {
    let temp_dir = Builder::new().prefix("legion-miette").tempdir().unwrap();
    let error = WorkspaceResolver::discover(temp_dir.path()).unwrap_err();
    let report = Report::new(error);
    let mut rendered = String::new();

    GraphicalReportHandler::new().with_links(false).with_urls(false).render_report(&mut rendered, report.as_ref()).unwrap();

    assert!(rendered.contains("legion::planner::missing_workspace"));
    assert!(rendered.contains("cannot locate `legions.von`"));
    assert!(rendered.contains("请在工作区根目录放置 `legions.von`"));
}

#[test]
fn resolves_root_manifest_without_workspace_as_script_mode() {
    let temp_dir = Builder::new().prefix("legion-script").tempdir().unwrap();
    fs::create_dir_all(temp_dir.path().join("source")).unwrap();
    fs::write(
        temp_dir.path().join("legion.von"),
        r#"{
    name: "script-app",
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        temp_dir.path().join("source").join("main.v"),
        r#"micro main() -> i64 {
    return 0;
}
"#,
    )
    .unwrap();

    let workspace = WorkspaceResolver::discover_for_project(temp_dir.path()).unwrap();
    assert!(workspace.workspace_manifest.is_none());

    let plan = workspace
        .build_plan(&BuildRequest { project_dir: temp_dir.path().to_path_buf(), target: CanonicalTarget::clr(), output_dir: None })
        .unwrap();

    assert_eq!(plan.project.name, "script-app");
    assert_eq!(canonicalize_lossy(&plan.project.manifest_dir), canonicalize_lossy(temp_dir.path()));
    assert!(plan.project.source_files.iter().any(|path| path.ends_with(Path::new("source").join("main.v"))));
}

#[test]
fn resolves_workspace_member_from_nested_source_directory() {
    let fixture = create_smoke_project_with_manifest(
        "legion-workspace-member-nested",
        r#"{
    name: "workspace-app",
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
        r#"micro main() -> i64 {
    return 0;
}
"#,
    );

    let workspace = WorkspaceResolver::discover_for_project(fixture.project_dir.join("source")).unwrap();
    let plan = workspace
        .build_plan(&BuildRequest { project_dir: fixture.project_dir.join("source"), target: CanonicalTarget::clr(), output_dir: None })
        .unwrap();

    assert_eq!(plan.project.name, "workspace-app");
    assert_eq!(canonicalize_lossy(&plan.project.manifest_dir), canonicalize_lossy(&fixture.project_dir));
}

#[test]
fn resolves_local_package_from_nested_source_directory() {
    let fixture = create_local_package_project_with_manifest(
        "legion-package-nested",
        r#"{
    name: "package-app",
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
        r#"micro main() -> i64 {
    return 0;
}
"#,
    );

    let workspace = WorkspaceResolver::discover_for_project(fixture.project_dir.join("source")).unwrap();
    let plan = workspace
        .build_plan(&BuildRequest { project_dir: fixture.project_dir.join("source"), target: CanonicalTarget::clr(), output_dir: None })
        .unwrap();

    assert_eq!(plan.project.name, "package-app");
    assert_eq!(canonicalize_lossy(&plan.project.manifest_dir), canonicalize_lossy(&fixture.project_dir));
}

#[test]
fn keeps_nested_workspace_member_layout_in_workspace_mode() {
    let fixture = create_nested_workspace_member_project_with_manifest(
        "legion-nested-workspace-member",
        r#"{
    name: "nested-workspace-app",
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
        r#"micro main() -> i64 {
    return 0;
}
"#,
    );

    let workspace = WorkspaceResolver::discover_for_project(fixture.project_dir.join("source")).unwrap();
    let plan = workspace
        .build_plan(&BuildRequest { project_dir: fixture.project_dir.join("source"), target: CanonicalTarget::clr(), output_dir: None })
        .unwrap();

    assert_eq!(plan.project.name, "nested-workspace-app");
    assert_eq!(canonicalize_lossy(&plan.project.manifest_dir), canonicalize_lossy(&fixture.project_dir));
}

#[test]
fn discovers_nested_workspace_members_from_parent_workspace() {
    let fixture = create_nested_workspace_member_project_with_manifest(
        "legion-parent-workspace-nested-member",
        r#"{
    name: "nested-member",
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
        r#"micro main() -> i64 {
    return 0;
}
"#,
    );
    let outer_workspace_root =
        fixture.project_dir.parent().and_then(|path| path.parent()).and_then(|path| path.parent()).unwrap().to_path_buf();

    let workspace = WorkspaceResolver::discover(&outer_workspace_root).unwrap();
    let plan = workspace
        .build_plan(&BuildRequest { project_dir: fixture.project_dir.clone(), target: CanonicalTarget::clr(), output_dir: None })
        .unwrap();

    assert_eq!(plan.project.name, "nested-member");
    assert_eq!(canonicalize_lossy(&plan.project.manifest_dir), canonicalize_lossy(&fixture.project_dir));
}

#[test]
fn nested_legion_tools_resolves_outer_core_via_outermost_workspace() {
    // Mirrors valkyrie.v layout:
    //   legions.von → projects/core + projects/legion._
    //   projects/legion._/legions.von → projects/legion.tools
    // Discovering from the nested project must not stop at legion._ (missing `core`).
    let fixture = create_nested_legion_tools_with_outer_core_fixture();

    let workspace = WorkspaceResolver::discover_for_project(&fixture.project_dir).unwrap();
    assert_eq!(canonicalize_lossy(&workspace.root_dir), canonicalize_lossy(&fixture.root_dir));

    for target in [CanonicalTarget::clr(), CanonicalTarget::jvm(), CanonicalTarget::parse("node").unwrap()] {
        let plan = workspace
            .build_plan(&BuildRequest { project_dir: fixture.project_dir.clone(), target, output_dir: None })
            .unwrap_or_else(|error| panic!("expected plan for {target}: {error}"));
        assert_eq!(plan.project.name, "legion.tools");
        assert!(
            plan.project.dependencies.iter().any(|dep| dep.name == "core"),
            "target {target} must resolve auto_link core from outer workspace"
        );
        assert_eq!(canonicalize_lossy(&plan.workspace_root), canonicalize_lossy(&fixture.root_dir));
    }
}

#[test]
fn builds_legion_tools_like_plan_with_explicit_dependency_closure() {
    let fixture = create_legion_tools_workspace_fixture();
    let workspace = WorkspaceResolver::discover(&fixture.project_dir).unwrap();
    let plan = workspace
        .build_plan(&BuildRequest { project_dir: fixture.project_dir.clone(), target: CanonicalTarget::clr(), output_dir: None })
        .unwrap();

    let dependency_names: Vec<&str> = plan.project.dependencies.iter().map(|item| item.name.as_str()).collect();
    assert_eq!(dependency_names, vec!["core", "legion", "std", "std.data.text.von"]);

    assert!(
        plan.project
            .source_files
            .iter()
            .any(|path| path.ends_with(Path::new("projects").join("legion.tools").join("source").join("build_context.v")))
    );
    assert!(plan.project.source_files.iter().any(|path| {
        path.ends_with(Path::new("projects").join("legion").join("source").join("types").join("targets").join("CanonicalTarget.v"))
    }));
    assert!(plan.project.source_files.iter().any(|path| path.ends_with(Path::new("projects").join("std").join("source").join("_.v"))));
    assert!(
        plan.project
            .source_files
            .iter()
            .any(|path| { path.ends_with(Path::new("projects").join("std.data.text.von").join("source").join("_.v")) })
    );
    assert!(!plan.project.source_files.iter().any(|path| path.to_string_lossy().contains("examples/test.module_system/test/")));
    assert!(!plan.project.source_files.iter().any(|path| path.to_string_lossy().contains("/test/")));
}

#[test]
fn prefers_workspace_dependency_under_auto_source() {
    let fixture = create_legion_tools_workspace_fixture();
    let workspace = WorkspaceResolver::discover(&fixture.project_dir).unwrap();
    let plan = workspace
        .build_plan(&BuildRequest { project_dir: fixture.project_dir.clone(), target: CanonicalTarget::clr(), output_dir: None })
        .unwrap();

    assert!(plan.project.dependencies.iter().any(|dep| dep.name == "legion"));
}

#[test]
fn reports_registry_dependency_without_version() {
    let temp_dir = Builder::new().prefix("legion-registry-source-missing-version").tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app");
    fs::create_dir_all(app_dir.join("source")).unwrap();
    fs::write(
        root.join("legions.von"),
        r#"{
    name: "planner-registry-source-missing-version",
    members: [
        "app"
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("legion.von"),
        r#"{
    name: "app",
    dependencies: {
        "left.pad": {
            source: "registry"
        }
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(app_dir.join("source").join("main.v"), "micro main() -> i64 { return 0; }\n").unwrap();
    let workspace = WorkspaceResolver::discover(&app_dir).unwrap();
    let error =
        workspace.build_plan(&BuildRequest { project_dir: app_dir.clone(), target: CanonicalTarget::clr(), output_dir: None }).unwrap_err();
    let report = Report::new(error);
    let mut rendered = String::new();
    GraphicalReportHandler::new().with_links(false).with_urls(false).render_report(&mut rendered, report.as_ref()).unwrap();
    assert!(rendered.contains("legion::planner::registry_dependency_missing_version"));
}

#[test]
fn reports_forced_workspace_dependency_missing() {
    let temp_dir = Builder::new().prefix("legion-forced-workspace-missing").tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app");
    fs::create_dir_all(app_dir.join("source")).unwrap();
    fs::write(
        root.join("legions.von"),
        r#"{
    name: "planner-forced-workspace-missing",
    members: [
        "app"
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("legion.von"),
        r#"{
    name: "app",
    dependencies: {
        "left.pad": {
            source: "workspace"
        }
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(app_dir.join("source").join("main.v"), "micro main() -> i64 { return 0; }\n").unwrap();
    let workspace = WorkspaceResolver::discover(&app_dir).unwrap();
    let error =
        workspace.build_plan(&BuildRequest { project_dir: app_dir.clone(), target: CanonicalTarget::clr(), output_dir: None }).unwrap_err();
    let report = Report::new(error);
    let mut rendered = String::new();
    GraphicalReportHandler::new().with_links(false).with_urls(false).render_report(&mut rendered, report.as_ref()).unwrap();
    assert!(rendered.contains("legion::planner::forced_workspace_dependency_missing"));
}

#[test]
fn collects_transitive_registry_dependency_sources() {
    let temp_dir = Builder::new().prefix("legion-registry-transitive").tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app");
    fs::create_dir_all(app_dir.join("source")).unwrap();

    let child_vendor = root.join("vendors").join("npm").join("child.lib@1.0.0");
    fs::create_dir_all(child_vendor.join("source")).unwrap();
    fs::write(
        child_vendor.join("legion.von"),
        r#"{
    name: "child.lib",
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(child_vendor.join("source").join("child.v"), "micro child_fn() -> i64 { return 1; }\n").unwrap();

    let parent_vendor = root.join("vendors").join("npm").join("parent.lib@2.0.0");
    fs::create_dir_all(parent_vendor.join("source")).unwrap();
    fs::write(
        parent_vendor.join("legion.von"),
        r#"{
    name: "parent.lib",
    dependencies: {
        "child.lib": "1.0.0"
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(parent_vendor.join("source").join("parent.v"), "micro parent_fn() -> i64 { return 2; }\n").unwrap();

    fs::write(
        root.join("legions.von"),
        r#"{
    name: "planner-registry-transitive",
    members: [
        "app"
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("legion.von"),
        r#"{
    name: "app",
    dependencies: {
        "parent.lib": {
            version: "2.0.0",
            source: "registry"
        }
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(app_dir.join("source").join("main.v"), "micro main() -> i64 { return 0; }\n").unwrap();

    let workspace = WorkspaceResolver::discover(&app_dir).unwrap();
    let plan = workspace.build_plan(&BuildRequest { project_dir: app_dir.clone(), target: CanonicalTarget::clr(), output_dir: None }).unwrap();

    assert!(plan.project.dependencies.iter().any(|dep| dep.name == "parent.lib"));
    assert!(plan.project.source_files.iter().any(|path| path.ends_with("parent.v")));
    assert!(plan.project.source_files.iter().any(|path| path.ends_with("child.v")));
}

#[test]
fn workspace_root_without_legion_manifest_is_detected() {
    let temp_dir = Builder::new().prefix("legion-workspace-only-root").tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app");
    fs::create_dir_all(app_dir.join("source")).unwrap();
    fs::write(
        root.join("legions.von"),
        r#"{
    name: "planner-workspace-only-root",
    members: [
        "app"
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("legion.von"),
        r#"{
    name: "app",
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(app_dir.join("source").join("main.v"), "micro main() -> i64 { return 0; }\n").unwrap();

    let workspace = WorkspaceResolver::discover(root).unwrap();
    assert!(workspace.is_workspace_only_root(root));
    assert_eq!(workspace.member_manifest_dirs().len(), 1);
}

#[test]
fn legion_tools_like_build_context_keeps_nyar_import_explicit() {
    let fixture = create_legion_tools_workspace_fixture();
    let build_context =
        fs::read_to_string(fixture.root_dir.join("projects").join("legion.tools").join("source").join("build_context.v")).unwrap();

    assert!(build_context.contains("using legion;"));
    assert!(build_context.contains("micro legion_parse_canonical_target(target: utf8) -> CanonicalTarget {"));
    assert!(build_context.contains("return parse_target(canonical)"));
    assert!(build_context.contains("return format_target(parsed)"));
}

fn create_legion_tools_workspace_fixture() -> WorkspaceFixture {
    let temp_dir = Builder::new().prefix("legion-tools-workspace").tempdir().unwrap();
    let root_dir = canonicalize_lossy(temp_dir.path());
    let project_dir = root_dir.join("projects").join("legion.tools");

    fs::create_dir_all(root_dir.join("projects")).unwrap();
    fs::write(
        root_dir.join("legions.von"),
        r#"{
    name: "planner-fixture",
    members: [
        "projects/core",
        "projects/legion",
        "projects/std",
        "projects/std.data.text.von",
        "projects/legion.tools"
    ]
}
"#,
    )
    .unwrap();

    write_workspace_project(
        &root_dir,
        "projects/core",
        r#"{
    name: "core",
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
        &[("source/_.v", "namespace core;\n")],
    );
    write_workspace_project(
        &root_dir,
        "projects/legion",
        r#"{
    name: "legion",
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
        &[("source/types/targets/CanonicalTarget.v", "namespace legion;\n")],
    );
    write_workspace_project(
        &root_dir,
        "projects/std",
        r#"{
    name: "std",
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
        &[("source/_.v", "namespace std;\n")],
    );
    write_workspace_project(
        &root_dir,
        "projects/std.data.text.von",
        r#"{
    name: "std.data.text.von",
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
        &[("source/_.v", "namespace std.data.text.von;\n")],
    );
    write_workspace_project(
        &root_dir,
        "projects/legion.tools",
        r#"{
    name: "legion.tools",
    auto_link: {
        core: true,
        std: false
    },
    dependencies: {
        "legion": { version: "workspace" },
        "std": { version: "workspace" },
        "std.data.text.von": { version: "workspace" }
    },
    build: [
        {
            target: "clr"
        },
        {
            target: "jvm"
        },
        {
            target: "node"
        },
        {
            target: "ovm"
        }
    ]
}
"#,
        &[(
            "source/build_context.v",
            r#"namespace legion.tools;
using legion;

micro legion_parse_canonical_target(target: utf8) -> CanonicalTarget {
    let canonical = legion_normalize_short_target(target)
    return parse_target(canonical)
}

micro legion_format_canonical_target(parsed: CanonicalTarget) -> utf8 {
    return format_target(parsed)
}

micro legion_normalize_short_target(value: utf8) -> utf8 {
    return value
}
"#,
        )],
    );

    WorkspaceFixture { _temp_dir: temp_dir, root_dir, project_dir }
}

fn create_nested_legion_tools_with_outer_core_fixture() -> WorkspaceFixture {
    let temp_dir = Builder::new().prefix("legion-tools-nested-outer-core").tempdir().unwrap();
    let root_dir = canonicalize_lossy(temp_dir.path());
    let subspace_dir = root_dir.join("projects").join("legion._");
    let project_dir = subspace_dir.join("projects").join("legion.tools");

    fs::create_dir_all(root_dir.join("projects")).unwrap();
    fs::create_dir_all(subspace_dir.join("projects")).unwrap();
    fs::write(
        root_dir.join("legions.von"),
        r#"{
    name: "outer-super-workspace",
    members: [
        "projects/core",
        "projects/legion._"
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        subspace_dir.join("legions.von"),
        r#"{
    name: "legion-super-workspace",
    members: [
        "projects/legion.tools"
    ]
}
"#,
    )
    .unwrap();

    write_workspace_project(
        &root_dir,
        "projects/core",
        r#"{
    name: "valkyrie-core",
    auto_link: {
        core: false,
        std: false
    },
    build: [
        {
            target: "ovm"
        }
    ]
}
"#,
        &[("source/_.v", "namespace core;\n")],
    );
    write_workspace_project(
        &subspace_dir,
        "projects/legion.tools",
        r#"{
    name: "legion.tools",
    auto_link: {
        core: true,
        std: false
    },
    dependencies: {},
    build: [
        {
            target: "clr"
        },
        {
            target: "jvm"
        },
        {
            target: "node"
        }
    ]
}
"#,
        &[("source/main.v", "micro main() -> i64 { return 0; }\n")],
    );

    WorkspaceFixture { _temp_dir: temp_dir, root_dir, project_dir: canonicalize_lossy(&project_dir) }
}

fn write_workspace_project(root_dir: &Path, relative_dir: &str, manifest: &str, sources: &[(&str, &str)]) {
    let project_dir = root_dir.join(relative_dir);
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(project_dir.join("legion.von"), manifest).unwrap();

    for (relative_file, content) in sources {
        let file_path = project_dir.join(relative_file);
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(file_path, content).unwrap();
    }
}

fn canonicalize_lossy(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[test]
fn filters_and_injects_sdk_vendor_by_publish_format() {
    let temp_dir = Builder::new().prefix("legion-publish-filter").tempdir().unwrap();
    let root = temp_dir.path();
    let app_game_dir = root.join("app.game");
    let app_web_dir = root.join("app.web");
    let app_mp_dir = root.join("app.mp");
    let wechat_sdk = root.join("tencent.wechat.sdk");
    let mp_sdk = root.join("tencent.wechat.miniprogram.sdk");

    for dir in [&app_game_dir, &app_web_dir, &app_mp_dir, &wechat_sdk, &mp_sdk] {
        fs::create_dir_all(dir.join("source")).unwrap();
    }

    fs::write(
        root.join("legions.von"),
        r#"{
    name: "planner-publish-filter",
    members: [
        "app.game",
        "app.web",
        "app.mp",
        "tencent.wechat.sdk",
        "tencent.wechat.miniprogram.sdk"
    ]
}
"#,
    )
    .unwrap();

    fs::write(
        app_game_dir.join("legion.von"),
        r#"{
    name: "app.game",
    build: [
        {
            target: "wasm",
            publish: ["mini-game"]
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(app_game_dir.join("source").join("main.v"), "micro main() -> i64 { return 0; }\n").unwrap();

    fs::write(
        app_web_dir.join("legion.von"),
        r#"{
    name: "app.web",
    dependencies: {
        "tencent.wechat.sdk": "workspace"
    },
    build: [
        {
            target: "wasm",
            publish: ["web-app"]
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(app_web_dir.join("source").join("main.v"), "micro main() -> i64 { return 0; }\n").unwrap();

    fs::write(
        app_mp_dir.join("legion.von"),
        r#"{
    name: "app.mp",
    build: [
        {
            target: "wasm",
            publish: ["mini-program"]
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(app_mp_dir.join("source").join("main.v"), "micro main() -> i64 { return 0; }\n").unwrap();

    fs::write(
        wechat_sdk.join("legion.von"),
        r#"{
    name: "tencent.wechat.sdk",
    sdk_vendor: {
        organization: "tencent",
        host: "wechat",
        kind: "platform-sdk",
        targets: ["wasm32-unknown-browser-wasm"],
        publish: ["mini-game"]
    },
    build: [{ target: "wasm" }]
}
"#,
    )
    .unwrap();
    fs::write(wechat_sdk.join("source").join("_.v"), "namespace tencent.wechat.sdk;\n").unwrap();

    fs::write(
        mp_sdk.join("legion.von"),
        r#"{
    name: "tencent.wechat.miniprogram.sdk",
    sdk_vendor: {
        organization: "tencent",
        host: "wechat-miniprogram",
        kind: "platform-sdk",
        targets: ["wasm32-unknown-browser-wasm"],
        publish: ["mini-program"]
    },
    build: [{ target: "wasm" }]
}
"#,
    )
    .unwrap();
    fs::write(mp_sdk.join("source").join("_.v"), "namespace tencent.wechat.miniprogram.sdk;\n").unwrap();

    let workspace = WorkspaceResolver::discover(&app_game_dir).unwrap();

    let game_plan =
        workspace.build_plan(&BuildRequest { project_dir: app_game_dir.clone(), target: CanonicalTarget::wasm(), output_dir: None }).unwrap();
    let game_deps: Vec<&str> = game_plan.project.dependencies.iter().map(|d| d.name.as_str()).collect();
    assert!(game_deps.contains(&"tencent.wechat.sdk"), "mini-game should inject wechat sdk: {game_deps:?}");
    assert!(!game_deps.contains(&"tencent.wechat.miniprogram.sdk"));
    assert_eq!(game_plan.project.build_target.publish, vec!["mini-game".to_string()]);

    let web_plan =
        workspace.build_plan(&BuildRequest { project_dir: app_web_dir.clone(), target: CanonicalTarget::wasm(), output_dir: None }).unwrap();
    let web_deps: Vec<&str> = web_plan.project.dependencies.iter().map(|d| d.name.as_str()).collect();
    assert!(!web_deps.contains(&"tencent.wechat.sdk"), "web-app must not see mini-game sdk: {web_deps:?}");
    assert!(!web_deps.contains(&"tencent.wechat.miniprogram.sdk"));

    let mp_plan =
        workspace.build_plan(&BuildRequest { project_dir: app_mp_dir.clone(), target: CanonicalTarget::wasm(), output_dir: None }).unwrap();
    let mp_deps: Vec<&str> = mp_plan.project.dependencies.iter().map(|d| d.name.as_str()).collect();
    assert!(mp_deps.contains(&"tencent.wechat.miniprogram.sdk"), "mini-program injects mp sdk: {mp_deps:?}");
    assert!(!mp_deps.contains(&"tencent.wechat.sdk"));
    assert_eq!(mp_plan.project.build_target.publish, vec!["mini-program".to_string()]);
}

#[test]
fn injects_distribution_default_adaptors_at_root_without_reverse_std_edge() {
    let temp_dir = Builder::new().prefix("legion-distribution-default").tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app");

    fs::write(
        root.join("legions.von"),
        r#"{
    name: "distribution-default-workspace",
    members: [
        "app",
        "core",
        "std",
        "std.adaptor.demo",
        "std.adaptor.disabled",
        "std.adaptor.clr"
    ]
}
"#,
    )
    .unwrap();

    write_workspace_project(
        root,
        "app",
        r#"{
    name: "app",
    dependencies: {
        "std": "workspace",
        "std.adaptor.disabled": false
    },
    build: [{ target: "ovm" }]
}
"#,
        &[("source/main.v", "micro main() -> i64 { return 0; }\n")],
    );
    write_workspace_project(
        root,
        "core",
        r#"{
    name: "core",
    build: [{ target: "ovm" }]
}
"#,
        &[("source/_.v", "namespace core;\n")],
    );
    write_workspace_project(
        root,
        "std",
        r#"{
    name: "std",
    dependencies: {},
    build: [{ target: "ovm" }]
}
"#,
        &[("source/_.v", "namespace std;\n")],
    );
    write_workspace_project(
        root,
        "std.adaptor.demo",
        r#"{
    name: "std.adaptor.demo",
    dependencies: {
        "core": "workspace",
        "std": "workspace"
    },
    sdk_vendor: {
        kind: "distribution-default",
        targets: ["ovm"]
    },
    build: [{ target: "ovm" }]
}
"#,
        &[("source/_.v", "namespace std.adaptor.demo;\n")],
    );
    write_workspace_project(
        root,
        "std.adaptor.disabled",
        r#"{
    name: "std.adaptor.disabled",
    sdk_vendor: {
        kind: "distribution-default",
        targets: ["ovm"]
    },
    build: [{ target: "ovm" }]
}
"#,
        &[("source/_.v", "namespace std.adaptor.disabled;\n")],
    );
    write_workspace_project(
        root,
        "std.adaptor.clr",
        r#"{
    name: "std.adaptor.clr",
    sdk_vendor: {
        kind: "distribution-default",
        targets: ["clr-microsoft-unknown-managed"]
    },
    build: [{ target: "clr-microsoft-unknown-managed" }]
}
"#,
        &[("source/_.v", "namespace std.adaptor.clr;\n")],
    );

    let workspace = WorkspaceResolver::discover(&app_dir).unwrap();
    let plan = workspace
        .build_plan(&BuildRequest { project_dir: app_dir.clone(), target: CanonicalTarget::parse("ovm").unwrap(), output_dir: None })
        .unwrap();

    let dependency_names = plan.project.dependencies.iter().map(|dependency| dependency.name.as_str()).collect::<Vec<_>>();
    assert!(dependency_names.contains(&"std.adaptor.demo"), "matching default adaptor must be injected: {dependency_names:?}");
    assert!(!dependency_names.contains(&"std.adaptor.disabled"), "explicit Disabled must not be overridden: {dependency_names:?}");
    assert!(!dependency_names.contains(&"std.adaptor.clr"), "non-matching target default must stay out: {dependency_names:?}");

    let group_index = |key: &str| {
        plan.project
            .semantic_source_groups
            .iter()
            .position(|group| group.dependency_key == key)
            .unwrap_or_else(|| panic!("missing semantic group `{key}`"))
    };
    let std_group = &plan.project.semantic_source_groups[group_index("std")];
    assert!(
        !std_group.direct_dependencies.iter().any(|dependency| dependency.starts_with("std.adaptor.")),
        "std must not depend back on adaptors: {:?}",
        std_group.direct_dependencies
    );

    let demo_group = &plan.project.semantic_source_groups[group_index("std.adaptor.demo")];
    assert!(demo_group.direct_dependencies.contains(&"std".to_string()), "adaptor must declare std visibility: {:?}", demo_group.direct_dependencies);
    assert!(group_index("std") < group_index("std.adaptor.demo"), "std must be lowered before its adaptor");
}

#[test]
fn unity_player_injects_sdk_and_prefers_unity_host_providers() {
    let temp_dir = Builder::new().prefix("legion-unknown-wasm32-player").tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app.unity");
    let unity_sdk = root.join("unity.engine.sdk");
    let clr_dir = root.join("std.adaptor.clr");
    let std_stub = root.join("std");

    for dir in [&app_dir, &unity_sdk, &clr_dir, &std_stub] {
        fs::create_dir_all(dir.join("source")).unwrap();
    }

    fs::write(
        root.join("legions.von"),
        r#"{
    name: "planner-unity-player",
    members: [
        "app.unity",
        "unity.engine.sdk",
        "std.adaptor.clr",
        "std"
    ]
}
"#,
    )
    .unwrap();

    fs::write(
        std_stub.join("legion.von"),
        r#"{
    name: "std",
    build: [{ target: "clr-microsoft-unknown-managed" }]
}
"#,
    )
    .unwrap();
    fs::write(
        std_stub.join("source").join("console.v"),
        r#"namespace std.console;
[host_contract]
micro write_line(message: utf8): unit
"#,
    )
    .unwrap();
    fs::write(
        std_stub.join("source").join("net.v"),
        r#"namespace std.net;
[host_contract]
micro get(url: utf8): utf8
"#,
    )
    .unwrap();

    fs::write(
        app_dir.join("legion.von"),
        r#"{
    name: "app.unity",
    dependencies: {
        "std": "workspace"
    },
    build: [
        {
            target: "clr-microsoft-unknown-managed",
            publish: ["unity-player"],
            msil: true
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(app_dir.join("source").join("main.v"), "micro main() { }\n").unwrap();

    fs::write(
        unity_sdk.join("legion.von"),
        r#"{
    name: "unity.engine.sdk",
    sdk_vendor: {
        organization: "unity",
        host: "engine",
        kind: "platform-sdk",
        targets: ["clr-microsoft-unknown-managed"],
        publish: ["unity-player"]
    },
    build: [{ target: "clr-microsoft-unknown-managed", msil: true }]
}
"#,
    )
    .unwrap();
    fs::write(
        unity_sdk.join("source").join("console.v"),
        r#"namespace unity.engine.sdk.console;
[host_provider(std::console::write_line)]
micro write_line(message: utf8): unit { }
"#,
    )
    .unwrap();
    fs::write(
        unity_sdk.join("source").join("net.v"),
        r#"namespace unity.engine.sdk.net;
[host_provider(std::net::get)]
micro get(url: utf8): utf8 { return ""; }
"#,
    )
    .unwrap();

    fs::write(
        clr_dir.join("legion.von"),
        r#"{
    name: "std.adaptor.clr",
    sdk_vendor: {
        organization: "valkyrie",
        host: "clr",
        kind: "platform-sdk",
        targets: ["clr-microsoft-unknown-managed"]
    },
    build: [{ target: "clr-microsoft-unknown-managed", msil: true }]
}
"#,
    )
    .unwrap();
    fs::write(
        clr_dir.join("source").join("console.v"),
        r#"namespace std.adaptor.clr.console;
[host_provider(std::console::write_line)]
micro write_line(message: utf8): unit { }
"#,
    )
    .unwrap();
    fs::write(
        clr_dir.join("source").join("net.v"),
        r#"namespace std.adaptor.clr.net;
[host_provider(std::net::get)]
micro get(url: utf8): utf8 { return ""; }
"#,
    )
    .unwrap();

    let workspace = WorkspaceResolver::discover(&app_dir).unwrap();
    let plan = workspace
        .build_plan(&BuildRequest {
            project_dir: app_dir.clone(),
            target: CanonicalTarget::parse("clr-microsoft-unknown-managed").unwrap(),
            output_dir: None,
        })
        .unwrap();

    let dep_names: Vec<&str> = plan.project.dependencies.iter().map(|dep| dep.name.as_str()).collect();
    assert!(dep_names.contains(&"unity.engine.sdk"), "unity-player should inject unity sdk: {dep_names:?}");

    let console_provider = plan
        .project
        .selected_host_providers
        .iter()
        .find(|provider| provider.contract.contains("console") && provider.contract.contains("write_line"))
        .expect("console write_line provider");
    assert!(
        console_provider.source_file.to_string_lossy().contains("unity.engine.sdk"),
        "unity-player must prefer unity.engine.sdk console provider"
    );
}

#[test]
fn filters_sdk_vendor_dependencies_by_target() {
    let temp_dir = Builder::new().prefix("legion-target-filter").tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app");
    let clr_dir = root.join("std.adaptor.clr");
    let wasm_dir = root.join("std.adaptor.wasm");

    fs::create_dir_all(app_dir.join("source")).unwrap();
    fs::create_dir_all(clr_dir.join("source")).unwrap();
    fs::create_dir_all(wasm_dir.join("source")).unwrap();

    fs::write(
        root.join("legions.von"),
        r#"{
    name: "planner-filter",
    members: [
        "app",
        "std.adaptor.clr",
        "std.adaptor.wasm"
    ]
}
"#,
    )
    .unwrap();

    fs::write(
        app_dir.join("legion.von"),
        r#"{
    name: "app",
    dependencies: {
        "std.adaptor.clr": "workspace",
        "std.adaptor.wasm": "workspace"
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("source").join("main.v"),
        r#"namespace demo;

[host_contract]
micro write(message: utf8): unit

micro main() -> i64 {
    return 0;
}
"#,
    )
    .unwrap();

    fs::write(
        clr_dir.join("legion.von"),
        r#"{
    name: "std.adaptor.clr",
    sdk_vendor: {
        organization: "valkyrie",
        host: "clr",
        kind: "platform-sdk",
        targets: ["clr"]
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        clr_dir.join("source").join("_.v"),
        r#"namespace std.adaptor.clr;

[host_provider(demo::write)]
micro write(message: utf8): unit {
}
"#,
    )
    .unwrap();

    fs::write(
        wasm_dir.join("legion.von"),
        r#"{
    name: "std.adaptor.wasm",
    sdk_vendor: {
        organization: "valkyrie",
        host: "browser",
        kind: "platform-sdk",
        targets: ["wasm"]
    },
    build: [
        {
            target: "wasm"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        wasm_dir.join("source").join("_.v"),
        r#"namespace std.adaptor.wasm;

[host_provider(demo::write)]
micro write(message: utf8): unit {
}
"#,
    )
    .unwrap();

    let workspace = WorkspaceResolver::discover(&app_dir).unwrap();
    let plan = workspace.build_plan(&BuildRequest { project_dir: app_dir.clone(), target: CanonicalTarget::clr(), output_dir: None }).unwrap();

    let dependency_names: Vec<&str> = plan.project.dependencies.iter().map(|item| item.name.as_str()).collect();
    assert_eq!(dependency_names, vec!["std.adaptor.clr"]);
    assert!(plan.project.source_files.iter().any(|path| path.ends_with(Path::new("std.adaptor.clr").join("source").join("_.v"))));
    assert!(!plan.project.source_files.iter().any(|path| path.ends_with(Path::new("std.adaptor.wasm").join("source").join("_.v"))));
    assert_eq!(plan.project.host_contracts.len(), 1);
    assert_eq!(plan.project.host_provider_candidates.len(), 1);
    assert_eq!(plan.project.selected_host_providers.len(), 1);
    assert_eq!(plan.project.selected_host_providers[0].contract, "demo::write");
    assert!(plan.project.selected_host_providers[0].source_file.ends_with(Path::new("std.adaptor.clr").join("source").join("_.v")));
}

#[test]
fn collects_host_contract_with_default_body_without_provider() {
    let temp_dir = Builder::new().prefix("legion-host-default").tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app");

    fs::create_dir_all(app_dir.join("source")).unwrap();

    fs::write(
        root.join("legions.von"),
        r#"{
    name: "planner-host-default",
    members: [
        "app"
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("legion.von"),
        r#"{
    name: "app",
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("source").join("main.v"),
        r#"namespace demo;

[host_contract]
micro write(message: utf8): unit {
    if message.length == 0 {
        return
    }
}
"#,
    )
    .unwrap();

    let workspace = WorkspaceResolver::discover(&app_dir).unwrap();
    let plan = workspace.build_plan(&BuildRequest { project_dir: app_dir.clone(), target: CanonicalTarget::clr(), output_dir: None }).unwrap();

    assert_eq!(plan.project.host_contracts.len(), 1);
    assert_eq!(plan.project.host_contracts[0].id, "demo::write");
    assert!(plan.project.host_provider_candidates.is_empty());
    assert!(plan.project.selected_host_providers.is_empty());
}

#[test]
fn collects_method_level_host_contract_and_provider_with_symbol_reference() {
    let temp_dir = Builder::new().prefix("legion-host-method").tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app");
    let sdk_dir = root.join("sdk");

    fs::create_dir_all(app_dir.join("source")).unwrap();
    fs::create_dir_all(sdk_dir.join("source")).unwrap();

    fs::write(
        root.join("legions.von"),
        r#"{
    name: "planner-host-method",
    members: [
        "app",
        "sdk"
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("legion.von"),
        r#"{
    name: "app",
    dependencies: {
        "sdk": "workspace"
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("source").join("main.v"),
        r#"namespace demo;

class Writer {}

imply Writer {
    [host_contract]
    micro write(self, message: utf8): unit {
        if message.length == 0 {
            return
        }
    }
}
"#,
    )
    .unwrap();
    fs::write(
        sdk_dir.join("legion.von"),
        r#"{
    name: "sdk",
    sdk_vendor: {
        organization: "valkyrie",
        host: "clr",
        kind: "platform-sdk",
        targets: ["clr"]
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        sdk_dir.join("source").join("_.v"),
        r#"namespace sdk;

imply demo.Writer {
    [host_provider(demo::Writer::write)]
    micro write(self, message: utf8): unit {
    }
}
"#,
    )
    .unwrap();

    let workspace = WorkspaceResolver::discover(&app_dir).unwrap();
    let plan = workspace.build_plan(&BuildRequest { project_dir: app_dir.clone(), target: CanonicalTarget::clr(), output_dir: None }).unwrap();

    assert_eq!(plan.project.host_contracts.len(), 1);
    assert_eq!(plan.project.host_contracts[0].id, "demo::Writer::write");
    assert_eq!(plan.project.host_provider_candidates.len(), 1);
    assert_eq!(plan.project.host_provider_candidates[0].contract, "demo::Writer::write");
    assert_eq!(plan.project.host_provider_candidates[0].symbol, "sdk::Writer::write");
    assert_eq!(plan.project.selected_host_providers.len(), 1);
}

#[test]
fn collects_host_provider_from_combined_attribute_list() {
    let temp_dir = Builder::new().prefix("legion-host-provider-attrs").tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app");
    let sdk_dir = root.join("sdk");

    fs::create_dir_all(app_dir.join("source")).unwrap();
    fs::create_dir_all(sdk_dir.join("source")).unwrap();

    fs::write(
        root.join("legions.von"),
        r#"{
    name: "planner-host-provider-attrs",
    members: [
        "app",
        "sdk"
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("legion.von"),
        r#"{
    name: "app",
    dependencies: {
        "sdk": "workspace"
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("source").join("main.v"),
        r#"namespace demo;

[host_contract]
micro clear(): unit
"#,
    )
    .unwrap();
    fs::write(
        sdk_dir.join("legion.von"),
        r#"{
    name: "sdk",
    sdk_vendor: {
        organization: "valkyrie",
        host: "clr",
        kind: "platform-sdk",
        targets: ["clr"]
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        sdk_dir.join("source").join("_.v"),
        r#"namespace sdk;

[host_provider(demo.clear), inline(always)]
micro clear(): unit {
}
"#,
    )
    .unwrap();

    let workspace = WorkspaceResolver::discover(&app_dir).unwrap();
    let plan = workspace.build_plan(&BuildRequest { project_dir: app_dir.clone(), target: CanonicalTarget::clr(), output_dir: None }).unwrap();

    assert_eq!(plan.project.host_contracts.len(), 1);
    assert_eq!(plan.project.host_contracts[0].id, "demo::clear");
    assert_eq!(plan.project.host_provider_candidates.len(), 1);
    assert_eq!(plan.project.host_provider_candidates[0].contract, "demo.clear");
    assert_eq!(plan.project.host_provider_candidates[0].symbol, "sdk::clear");
    assert_eq!(plan.project.selected_host_providers.len(), 1);
}

#[test]
fn keeps_legacy_string_host_provider_attribute_compatible() {
    let temp_dir = Builder::new().prefix("legion-host-provider-string").tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app");
    let sdk_dir = root.join("sdk");

    fs::create_dir_all(app_dir.join("source")).unwrap();
    fs::create_dir_all(sdk_dir.join("source")).unwrap();

    fs::write(
        root.join("legions.von"),
        r#"{
    name: "planner-host-provider-string",
    members: [
        "app",
        "sdk"
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("legion.von"),
        r#"{
    name: "app",
    dependencies: {
        "sdk": "workspace"
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("source").join("main.v"),
        r#"namespace demo;

[host_contract]
micro clear(): unit
"#,
    )
    .unwrap();
    fs::write(
        sdk_dir.join("legion.von"),
        r#"{
    name: "sdk",
    sdk_vendor: {
        organization: "valkyrie",
        host: "clr",
        kind: "platform-sdk",
        targets: ["clr"]
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        sdk_dir.join("source").join("_.v"),
        r#"namespace sdk;

[host_provider("demo.clear")]
micro clear(): unit {
}
"#,
    )
    .unwrap();

    let workspace = WorkspaceResolver::discover(&app_dir).unwrap();
    let plan = workspace.build_plan(&BuildRequest { project_dir: app_dir.clone(), target: CanonicalTarget::clr(), output_dir: None }).unwrap();

    assert_eq!(plan.project.host_contracts.len(), 1);
    assert_eq!(plan.project.host_contracts[0].id, "demo::clear");
    assert_eq!(plan.project.host_provider_candidates.len(), 1);
    assert_eq!(plan.project.host_provider_candidates[0].contract, "demo.clear");
    assert_eq!(plan.project.host_provider_candidates[0].symbol, "sdk::clear");
    assert_eq!(plan.project.selected_host_providers.len(), 1);
}

#[test]
fn reports_conflicting_host_providers_during_planning() {
    let temp_dir = Builder::new().prefix("legion-host-conflict").tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app");
    let left_dir = root.join("sdk.left");
    let right_dir = root.join("sdk.right");

    fs::create_dir_all(app_dir.join("source")).unwrap();
    fs::create_dir_all(left_dir.join("source")).unwrap();
    fs::create_dir_all(right_dir.join("source")).unwrap();

    fs::write(
        root.join("legions.von"),
        r#"{
    name: "planner-host-conflict",
    members: [
        "app",
        "sdk.left",
        "sdk.right"
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("legion.von"),
        r#"{
    name: "app",
    dependencies: {
        "sdk.left": "workspace",
        "sdk.right": "workspace"
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("source").join("main.v"),
        r#"namespace demo;

[host_contract]
micro write(message: utf8): unit
"#,
    )
    .unwrap();

    for project_dir in [&left_dir, &right_dir] {
        fs::write(
            project_dir.join("legion.von"),
            r#"{
    name: "sdk",
    sdk_vendor: {
        organization: "valkyrie",
        host: "clr",
        kind: "platform-sdk",
        targets: ["clr"]
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
        )
        .unwrap();
    }

    fs::write(
        left_dir.join("legion.von"),
        r#"{
    name: "sdk.left",
    sdk_vendor: {
        organization: "valkyrie",
        host: "clr",
        kind: "platform-sdk",
        targets: ["clr"]
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        right_dir.join("legion.von"),
        r#"{
    name: "sdk.right",
    sdk_vendor: {
        organization: "valkyrie",
        host: "clr",
        kind: "platform-sdk",
        targets: ["clr"]
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();

    fs::write(
        left_dir.join("source").join("_.v"),
        r#"namespace sdk.left;

[host_provider(demo::write)]
micro write(message: utf8): unit {
}
"#,
    )
    .unwrap();
    fs::write(
        right_dir.join("source").join("_.v"),
        r#"namespace sdk.right;

[host_provider(demo::write)]
micro write(message: utf8): unit {
}
"#,
    )
    .unwrap();

    let workspace = WorkspaceResolver::discover(&app_dir).unwrap();
    let plan = workspace.build_plan(&BuildRequest { project_dir: app_dir.clone(), target: CanonicalTarget::clr(), output_dir: None }).unwrap();

    assert_eq!(plan.project.host_contracts.len(), 1);
    assert_eq!(plan.project.host_contracts[0].id, "demo::write");
    assert_eq!(plan.project.host_provider_candidates.len(), 2);
    assert_eq!(plan.project.selected_host_providers.len(), 1);
    assert_eq!(plan.project.selected_host_providers[0].contract, "demo::write");
    assert!(
        ["sdk::left::write", "sdk::right::write"].contains(&plan.project.selected_host_providers[0].symbol.as_str()),
        "expected deterministic host-provider tie-break, got {}",
        plan.project.selected_host_providers[0].symbol
    );
}

#[test]
fn reports_unknown_host_provider_contract_during_planning() {
    let temp_dir = Builder::new().prefix("legion-host-unknown-contract").tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app");
    let sdk_dir = root.join("sdk");

    fs::create_dir_all(app_dir.join("source")).unwrap();
    fs::create_dir_all(sdk_dir.join("source")).unwrap();

    fs::write(
        root.join("legions.von"),
        r#"{
    name: "planner-host-unknown",
    members: [
        "app",
        "sdk"
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("legion.von"),
        r#"{
    name: "app",
    dependencies: {
        "sdk": "workspace"
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join("source").join("main.v"),
        r#"namespace demo;

[host_contract]
micro write(message: utf8): unit
"#,
    )
    .unwrap();
    fs::write(
        sdk_dir.join("legion.von"),
        r#"{
    name: "sdk",
    sdk_vendor: {
        organization: "valkyrie",
        host: "clr",
        kind: "platform-sdk",
        targets: ["clr"]
    },
    build: [
        {
            target: "clr"
        }
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        sdk_dir.join("source").join("_.v"),
        r#"namespace sdk;

[host_provider(demo.missing)]
micro write(message: utf8): unit {
}
"#,
    )
    .unwrap();

    let workspace = WorkspaceResolver::discover(&app_dir).unwrap();
    let plan = workspace.build_plan(&BuildRequest { project_dir: app_dir.clone(), target: CanonicalTarget::clr(), output_dir: None }).unwrap();

    assert_eq!(plan.project.host_contracts.len(), 1);
    assert_eq!(plan.project.host_contracts[0].id, "demo::write");
    assert_eq!(plan.project.host_provider_candidates.len(), 1);
    assert_eq!(plan.project.host_provider_candidates[0].contract, "demo.missing");
    assert!(plan.project.selected_host_providers.is_empty());
}

#[test]
fn resolves_path_dependency_outside_workspace() {
    let temp_dir = Builder::new().prefix("legion-path-dep").tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app");
    let std_dir = root.join("stdlib");
    fs::create_dir_all(app_dir.join("source")).unwrap();
    fs::create_dir_all(std_dir.join("source")).unwrap();
    fs::write(
        app_dir.join("legion.von"),
        r#"{
    name: "app",
    dependencies: {
        "std": {
            source: "path",
            path: "../stdlib"
        }
    },
    build: [
        { target: "node" }
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        std_dir.join("legion.von"),
        r#"{
    name: "std",
    build: [
        { target: "node" }
    ]
}
"#,
    )
    .unwrap();
    fs::write(std_dir.join("source").join("lib.v"), "namespace std.lib;\n").unwrap();
    fs::write(app_dir.join("source").join("main.v"), "micro main() -> i64 { return 0; }\n").unwrap();

    let workspace = WorkspaceResolver::discover_for_project(&app_dir).unwrap();
    let plan = workspace
        .build_plan(&BuildRequest { project_dir: app_dir.clone(), target: CanonicalTarget::parse("node").unwrap(), output_dir: None })
        .unwrap();

    assert!(plan.project.dependencies.iter().any(|dep| dep.name == "std"));
    assert!(plan.project.source_files.iter().any(|path| path.ends_with("stdlib/source/lib.v")));
}

#[test]
fn local_config_overrides_git_dependency_with_path() {
    let temp_dir = Builder::new().prefix("legion-local-config").tempdir().unwrap();
    let root = temp_dir.path();
    let app_dir = root.join("app");
    let std_dir = root.join("stdlib");
    fs::create_dir_all(app_dir.join("source")).unwrap();
    fs::create_dir_all(app_dir.join(".config/legion")).unwrap();
    fs::create_dir_all(std_dir.join("source")).unwrap();
    fs::write(
        app_dir.join("legion.von"),
        r#"{
    name: "app",
    dependencies: {
        "std": {
            source: "git",
            git: "https://example.com/valkyrie.v.git",
            ref: "main",
            path: "projects/std"
        }
    },
    build: [
        { target: "node" }
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        app_dir.join(".config/legion/legions.von"),
        r#"{
    name: "local-overrides",
    dependencies: {
        "std": {
            source: "path",
            path: "../stdlib"
        }
    }
}
"#,
    )
    .unwrap();
    fs::write(
        std_dir.join("legion.von"),
        r#"{
    name: "std",
    build: [
        { target: "node" }
    ]
}
"#,
    )
    .unwrap();
    fs::write(std_dir.join("source").join("lib.v"), "namespace std.lib;\n").unwrap();
    fs::write(app_dir.join("source").join("main.v"), "micro main() -> i64 { return 0; }\n").unwrap();

    let workspace = WorkspaceResolver::discover_for_project(&app_dir).unwrap();
    let plan = workspace
        .build_plan(&BuildRequest { project_dir: app_dir.clone(), target: CanonicalTarget::parse("node").unwrap(), output_dir: None })
        .unwrap();

    assert!(plan.project.dependencies.iter().any(|dep| dep.name == "std"));
    assert!(plan.project.source_files.iter().any(|path| path.ends_with("stdlib/source/lib.v")));
}
