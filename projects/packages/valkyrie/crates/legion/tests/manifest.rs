use legion::{
    CanonicalTarget, DependencySpec, ProjectArtifactKind, ProjectManifest, PublishFormat, RunnerFamily, RunnerSelector, WorkspaceManifest,
};

fn is_workspace_like_dependency(spec: &DependencySpec) -> bool {
    match spec {
        DependencySpec::Workspace => true,
        DependencySpec::Detailed { version: Some(version), path: None, abi: None, source: None, registry: None, git: None, git_ref: None } => {
            version == "workspace"
        }
        _ => false,
    }
}

#[test]
fn parses_project_manifest_artifact_library() {
    let source = r#"
    {
        name: "demo.two_sum",
        artifact: "library",
        build: [{ target: "node" }]
    }
    "#;

    let manifest = ProjectManifest::parse(source).unwrap();
    assert_eq!(manifest.artifact, ProjectArtifactKind::Library);
}

#[test]
fn parses_project_manifest_dependencies() {
    let source = r#"
    {
        name: "test.hello_world",
        dependencies: {
            "std": true,
            "std.data.text.von": {
                version: "workspace",
                abi: "clr"
            }
        },
        build: [
            {
                target: "clr"
            }
        ]
    }
    "#;

    let manifest = ProjectManifest::parse(source).unwrap();
    assert_eq!(manifest.name, "test.hello_world");
    assert_eq!(manifest.build.len(), 1);
    assert_eq!(manifest.build[0].target, CanonicalTarget::clr());
    assert_eq!(manifest.dependencies.get("std"), Some(&DependencySpec::Workspace));
}

#[test]
fn parses_workspace_members() {
    let source = r#"
    {
        name: "workspace",
        members: [
            "examples/test.io",
            "projects/std"
        ],
        workspace: {
            version: "0.1.0"
        }
    }
    "#;

    let manifest = WorkspaceManifest::parse(source).unwrap();
    assert_eq!(manifest.members.len(), 2);
    assert_eq!(manifest.workspace.version.as_deref(), Some("0.1.0"));
}

#[test]
fn parses_workspace_runner_bindings() {
    let source = r#"
    {
        members: [
            "examples/test.io"
        ],
        runner: [
            {
                target: "clr",
                command: "dotnet",
                args: ["exec", "{artifact}"]
            },
            {
                target: "node",
                command: "node",
                args: ["{artifact}"]
            }
        ]
    }
    "#;

    let manifest = WorkspaceManifest::parse(source).unwrap();
    assert_eq!(manifest.runner.len(), 2);
    assert_eq!(manifest.runner[0].target, RunnerSelector::Family(RunnerFamily::Clr));
    assert_eq!(manifest.runner[1].command, "node");
}

#[test]
fn parses_publish_formats() {
    let source = r#"
    {
        name: "test.publish",
        publish: [
            {
                target: "wasm",
                type: "web-app"
            }
        ]
    }
    "#;

    let manifest = ProjectManifest::parse(source).unwrap();
    assert_eq!(manifest.publish.len(), 1);
    assert_eq!(manifest.publish[0].artifact_publish_format(), Some(PublishFormat::WebApp));
}

#[test]
fn parses_registry_publish_targets() {
    let source = r#"
    {
        name: "legion.tools",
        publish: [
            {
                target: "node",
                type: "npm",
                package_id: "@valkyrie-language/legion",
                version: "2020.0.0"
            }
        ]
    }
    "#;

    let manifest = ProjectManifest::parse(source).unwrap();
    assert_eq!(manifest.publish.len(), 1);
    assert_eq!(manifest.publish[0].channel_type.as_deref(), Some("npm"));
    assert_eq!(manifest.publish[0].package_id.as_deref(), Some("@valkyrie-language/legion"));
    assert_eq!(manifest.publish[0].version.as_deref(), Some("2020.0.0"));
    assert_eq!(manifest.publish[0].artifact_publish_format(), None);
}

#[test]
fn parses_build_target_publish_formats() {
    let source = r#"
    {
        name: "demo.wechat.game",
        build: [
            {
                target: "wasm32-unknown-browser-wasm",
                publish: ["mini-game", "mini-program"],
                source_map: true
            }
        ]
    }
    "#;
    let manifest = ProjectManifest::parse(source).unwrap();
    assert_eq!(manifest.build.len(), 1);
    assert_eq!(manifest.build[0].publish, vec!["mini-game".to_string(), "mini-program".to_string()]);

    let apk_source = r#"
    {
        name: "demo.android",
        build: [
            {
                target: "jvm-android-android-managed",
                publish: ["apk"]
            }
        ]
    }
    "#;
    let apk_manifest = ProjectManifest::parse(apk_source).unwrap();
    assert_eq!(apk_manifest.build[0].publish, vec!["apk".to_string()]);

    let ipa_source = r#"
    {
        name: "demo.ios",
        build: [
            {
                target: "aarch64-apple-ios-aapcs64",
                publish: ["ipa"]
            }
        ]
    }
    "#;
    let ipa_manifest = ProjectManifest::parse(ipa_source).unwrap();
    assert_eq!(ipa_manifest.build[0].publish, vec!["ipa".to_string()]);
}

#[test]
fn parses_sdk_vendor_metadata() {
    let source = r#"
    {
        name: "std.adaptor.wasm",
        sdk_vendor: {
            organization: "valkyrie",
            host: "browser",
            kind: "platform-sdk",
            targets: ["wasm32-unknown-browser-wasm"],
            publish: ["web-app"]
        }
    }
    "#;

    let manifest = ProjectManifest::parse(source).unwrap();
    let sdk_vendor = manifest.sdk_vendor.as_ref().unwrap();
    assert_eq!(sdk_vendor.organization.as_deref(), Some("valkyrie"));
    assert_eq!(sdk_vendor.host.as_deref(), Some("browser"));
    assert_eq!(sdk_vendor.kind.as_deref(), Some("platform-sdk"));
    assert_eq!(sdk_vendor.targets, vec!["wasm32-unknown-browser-wasm".to_string()]);
    assert_eq!(sdk_vendor.publish, vec!["web-app".to_string()]);
}

#[test]
fn parses_legion_tools_like_manifest_with_explicit_module_dependencies() {
    let source = r#"
    {
        name: "legion.tools",
        version: "workspace",
        description: "Legion 构造工具",
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
                target: "wasm"
            },
            {
                target: "ovm"
            }
        ]
    }
    "#;
    let manifest = ProjectManifest::parse(&source).unwrap();

    assert_eq!(manifest.name, "legion.tools");
    assert!(manifest.auto_link.core);
    assert!(!manifest.auto_link.std);

    assert!(is_workspace_like_dependency(manifest.dependencies.get("legion").unwrap()));
    assert!(is_workspace_like_dependency(manifest.dependencies.get("std").unwrap()));
    assert!(is_workspace_like_dependency(manifest.dependencies.get("std.data.text.von").unwrap()));

    assert!(manifest.build.iter().any(|item| item.target == CanonicalTarget::clr()));
    assert!(manifest.build.iter().any(|item| item.target == CanonicalTarget::parse("jvm").unwrap()));
    assert!(manifest.build.iter().any(|item| item.target == CanonicalTarget::parse("wasm").unwrap()));
    assert!(manifest.build.iter().any(|item| item.target == CanonicalTarget::parse("ovm").unwrap()));
}

#[test]
fn parses_actual_workspace_members_after_examples_are_narrowed() {
    let source = r#"
    {
        name: "valkyrie-super-workspace",
        members: [
            "examples/demo.wechat.game",
            "examples/demo.unity.game",
            "projects/legion",
            "projects/std",
            "projects/legion.tools"
        ],
        workspace: {
            version: "0.1.0",
            auto_link: {
                core: false,
                std: false
            }
        }
    }
    "#;
    let manifest = WorkspaceManifest::parse(&source).unwrap();

    assert!(manifest.members.contains(&"projects/legion.tools".to_string()));
    assert!(manifest.members.contains(&"projects/legion".to_string()));
    assert!(manifest.members.contains(&"projects/std".to_string()));
    assert!(manifest.members.contains(&"examples/demo.unity.game".to_string()));
    assert!(!manifest.members.contains(&"examples/test.module_system".to_string()));
    assert!(!manifest.workspace.auto_link.core);
    assert!(!manifest.workspace.auto_link.std);
}

#[test]
fn parses_build_plugin() {
    let source = r#"
    {
        name: "demo.unity.game",
        build_plugin: {
            kind: "unity-project-export",
            sdk: "unity.engine.sdk",
            mode: "auto",
            input_directory: "build/unity/msil",
            output_directory: "build/unity/project"
        }
    }
    "#;
    let manifest = ProjectManifest::parse(source).unwrap();
    let plugin = manifest.build_plugin.expect("build_plugin should parse");
    assert_eq!(plugin.kind, "unity-project-export");
    assert_eq!(plugin.sdk.as_deref(), Some("unity.engine.sdk"));
    assert_eq!(plugin.input_directory.as_deref(), Some("build/unity/msil"));
    assert_eq!(plugin.output_directory.as_deref(), Some("build/unity/project"));
}

#[test]
fn parses_build_plugin_export_routes() {
    let source = r#"
    {
        name: "valkyrie.unity",
        build_plugin: {
            kind: "unity-project-export",
            export_routes: {
                "unity.runtime": "../../../../valkyrie.unity/Runtime",
                "unity.editor": "../../../../valkyrie.unity/Editor"
            }
        }
    }
    "#;
    let manifest = ProjectManifest::parse(source).unwrap();
    let plugin = manifest.build_plugin.expect("build_plugin should parse");
    assert_eq!(plugin.export_routes.get("unity.runtime").map(String::as_str), Some("../../../../valkyrie.unity/Runtime"));
}

#[test]
fn parses_dependency_source_registry_and_workspace() {
    let source = r#"
    {
        name: "demo",
        dependencies: {
            "foo": {
                version: "1.2.3",
                source: "registry"
            },
            "bar": {
                source: "workspace"
            }
        }
    }
    "#;
    let manifest = ProjectManifest::parse(source).unwrap();
    assert!(matches!(
        manifest.dependencies.get("foo"),
        Some(DependencySpec::Detailed { version: Some(version), source: Some(dep_source), .. })
            if version == "1.2.3" && dep_source == "registry"
    ));
    assert!(matches!(
        manifest.dependencies.get("bar"),
        Some(DependencySpec::Detailed { source: Some(dep_source), .. }) if dep_source == "workspace"
    ));
}

#[test]
fn parses_git_and_path_dependencies() {
    let source = r#"
    {
        name: "demo",
        dependencies: {
            "std": {
                source: "path",
                path: "../valkyrie.v/projects/std"
            },
            "core": {
                source: "git",
                git: "https://github.com/valkyrie-language/valkyrie.v.git",
                ref: "main",
                path: "projects/core"
            }
        }
    }
    "#;
    let manifest = ProjectManifest::parse(source).unwrap();
    let std = manifest.dependencies.get("std").unwrap();
    assert_eq!(std.source_preference(), legion_workspace::manifest::DependencySourcePreference::Path);
    assert_eq!(std.path_hint(), Some("../valkyrie.v/projects/std"));
    let core = manifest.dependencies.get("core").unwrap();
    assert_eq!(core.source_preference(), legion_workspace::manifest::DependencySourcePreference::Git);
    assert_eq!(core.git_url(), Some("https://github.com/valkyrie-language/valkyrie.v.git"));
    assert_eq!(core.git_ref_hint(), Some("main"));
    assert_eq!(core.path_hint(), Some("projects/core"));
}
