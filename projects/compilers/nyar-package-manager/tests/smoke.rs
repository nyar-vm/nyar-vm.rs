use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};

use nyar_package_manager::{MockRegistry, Package, PackageManager, PublishOptions, Registry, RegistrySourceManager, SecurityAudit, unpack};
use tempfile::TempDir;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn with_isolated_home<T>(f: impl FnOnce(&Path) -> T) -> T {
    let _guard = env_lock().lock().expect("env lock");
    let home = TempDir::new().expect("temp home");
    let previous = std::env::var_os("NYAR_HOME");
    // SAFETY: guarded by process-wide mutex for these tests.
    unsafe {
        std::env::set_var("NYAR_HOME", home.path());
    }
    let result = f(home.path());
    unsafe {
        match previous {
            Some(value) => std::env::set_var("NYAR_HOME", value),
            None => std::env::remove_var("NYAR_HOME"),
        }
    }
    result
}

fn write_package_fixture(dir: &Path, name: &str, version: &str) {
    std::fs::write(
        dir.join("package.von"),
        format!(
            "{{\n    name: \"{name}\",\n    version: \"{version}\",\n    description: \"smoke package\",\n    publishConfig: {{\n        registry: \"npm\",\n        access: \"public\",\n        tag: \"latest\",\n    }},\n}}\n"
        ),
    )
    .expect("write package.von");
    std::fs::write(dir.join("source.txt"), "hello").expect("write source");
}

fn tarball_package_json(tarball: &[u8]) -> String {
    let dir = tempfile::tempdir().expect("temp unpack");
    unpack(tarball, dir.path()).expect("unpack");
    std::fs::read_to_string(dir.path().join("package.json")).expect("package.json")
}

fn write_publish_target_fixture(dir: &Path) {
    let artifact_dir = dir.join("dist").join("wasm32-node-unknown-wasm");
    std::fs::create_dir_all(&artifact_dir).expect("artifact dir");
    std::fs::write(artifact_dir.join("demo_cli.mjs"), "export async function run() {}").expect("mjs");
    std::fs::write(artifact_dir.join("demo_cli.wasm"), b"\0asm").expect("wasm");
    std::fs::write(artifact_dir.join("demo_tool.mjs"), "export async function run() {}").expect("tool mjs");
    std::fs::write(artifact_dir.join("demo_tool.wasm"), b"\0asm").expect("tool wasm");
    std::fs::write(
        artifact_dir.join("run-contracts.txt"),
        r#"{
    run_contracts: [
        {
            logical_entry: "demo::cli",
            physical_entry: "demo_cli.mjs",
            invocation: "node",
            validate: "node demo_cli.mjs"
        },
        {
            logical_entry: "demo::tool",
            physical_entry: "demo_tool.mjs",
            invocation: "node",
            validate: "node demo_tool.mjs"
        }
    ]
}
"#,
    )
    .expect("contracts");
    std::fs::write(
        dir.join("package.von"),
        r#"{
    name: "smoke.tools",
    version: "workspace",
    description: "smoke publish-target package",
    publish: [
        {
            target: "node",
            type: "npm",
            package_id: "@scope/demo-cli",
            version: "2020.0.0"
        }
    ]
}
"#,
    )
    .expect("write package.von");
}

fn mock_registries(mock: Arc<MockRegistry>) -> HashMap<String, Arc<dyn Registry>> {
    let mut registries: HashMap<String, Arc<dyn Registry>> = HashMap::new();
    registries.insert(mock.name().to_string(), mock);
    registries
}

#[test]
fn publish_dry_run_packs_without_upload() {
    with_isolated_home(|_| {
        let package_dir = TempDir::new().expect("package dir");
        write_package_fixture(package_dir.path(), "smoke.publish", "0.1.0");

        let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
        let mut pm = PackageManager::open_with_registries(
            package_dir.path(),
            mock_registries(mock.clone()),
            nyar_package_manager::ProjectLayout::neutral(),
        )
        .expect("open package manager");

        let result = pm
            .publish(
                PublishOptions {
                    dry_run: true,
                    skip_git_check: true,
                    create_git_tag: false,
                    run_pre_publish_script: false,
                    registry_name: "npm".to_string(),
                    ..PublishOptions::default()
                },
                false,
            )
            .expect("dry-run publish");

        assert!(result.success);
        assert!(result.dry_run);
        assert_eq!(result.package_name, "smoke.publish");
        assert_eq!(result.version, "0.1.0");
        assert!(result.size.unwrap_or(0) > 0);
        assert!(result.file_count.unwrap_or(0) > 0);
        assert!(result.sha256.as_deref().unwrap_or("").starts_with("sha256-"));
        assert!(mock.published().is_empty());
    });
}

#[test]
fn publish_uploads_tarball_to_mock_registry() {
    with_isolated_home(|_| {
        let package_dir = TempDir::new().expect("package dir");
        write_package_fixture(package_dir.path(), "smoke.upload", "1.2.3");

        let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
        let mut pm = PackageManager::open_with_registries(
            package_dir.path(),
            mock_registries(mock.clone()),
            nyar_package_manager::ProjectLayout::neutral(),
        )
        .expect("open package manager");

        let result = pm
            .publish(
                PublishOptions {
                    dry_run: false,
                    skip_git_check: true,
                    create_git_tag: false,
                    run_pre_publish_script: false,
                    registry_name: "npm".to_string(),
                    ..PublishOptions::default()
                },
                false,
            )
            .expect("publish");

        assert!(result.success);
        assert!(!result.dry_run);
        assert_eq!(result.package_name, "smoke.upload");
        assert_eq!(result.version, "1.2.3");

        let published = mock.published();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].0, "smoke.upload");
        assert_eq!(published[0].1, "1.2.3");
        assert!(!published[0].2.is_empty());
    });
}

#[test]
fn publish_uses_manifest_publish_target_package_id() {
    with_isolated_home(|_| {
        let package_dir = TempDir::new().expect("package dir");
        write_publish_target_fixture(package_dir.path());

        let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
        let mut pm = PackageManager::open_with_registries(
            package_dir.path(),
            mock_registries(mock.clone()),
            nyar_package_manager::ProjectLayout::neutral(),
        )
        .expect("open package manager");

        let result = pm
            .publish(
                PublishOptions {
                    dry_run: false,
                    skip_git_check: true,
                    create_git_tag: false,
                    run_pre_publish_script: false,
                    ..PublishOptions::default()
                },
                false,
            )
            .expect("publish");

        assert!(result.success);
        assert_eq!(result.package_name, "@scope/demo-cli");
        assert_eq!(result.version, "2020.0.0");

        let published = mock.published();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].0, "@scope/demo-cli");
        assert_eq!(published[0].1, "2020.0.0");

        let package_json = tarball_package_json(&published[0].2);
        assert!(package_json.contains("demo_cli.mjs"));
        assert!(package_json.contains("demo_tool.mjs"));
        assert!(package_json.contains("\"cli\""));
        assert!(package_json.contains("\"tool\""));
    });
}

#[test]
fn publish_uses_node_dist_not_project_root() {
    with_isolated_home(|_| {
        let package_dir = TempDir::new().expect("package dir");
        write_publish_target_fixture(package_dir.path());
        std::fs::write(package_dir.path().join("source.txt"), "secret").expect("source");

        let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
        let mut pm = PackageManager::open_with_registries(
            package_dir.path(),
            mock_registries(mock.clone()),
            nyar_package_manager::ProjectLayout::neutral(),
        )
        .expect("open package manager");

        let result = pm
            .publish(
                PublishOptions {
                    dry_run: true,
                    skip_git_check: true,
                    create_git_tag: false,
                    run_pre_publish_script: false,
                    ..PublishOptions::default()
                },
                false,
            )
            .expect("dry-run publish");

        assert!(result.success);
        assert!(result.dry_run);
        assert!(result.file_count.unwrap_or(0) <= 8);
    });
}

#[test]
fn install_writes_vendors_and_lockfile() {
    with_isolated_home(|_| {
        let package_dir = TempDir::new().expect("package dir");
        write_package_fixture(package_dir.path(), "smoke.root", "0.1.0");

        let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
        mock.insert_package(Package {
            name: "smoke.dep".to_string(),
            version: "2.0.0".to_string(),
            description: "dep".to_string(),
            ..Package::default()
        });

        let mut pm =
            PackageManager::open_with_registries(package_dir.path(), mock_registries(mock), nyar_package_manager::ProjectLayout::neutral())
                .expect("open package manager");
        let info = pm.install_one("smoke.dep", "2.0.0", "npm", true).expect("install");

        assert_eq!(info.name, "smoke.dep");
        assert_eq!(info.version, "2.0.0");

        let vendor_path = package_dir.path().join("vendors").join("npm").join("smoke.dep@2.0.0");
        assert!(vendor_path.is_dir(), "vendors path missing: {}", vendor_path.display());
        assert!(vendor_path.join("package.von").is_file());

        let lock_path = package_dir.path().join("package-lock.von");
        assert!(lock_path.is_file());
        let lock = std::fs::read_to_string(lock_path).expect("read lock");
        assert!(lock.contains("smoke.dep"));
        assert!(lock.contains("2.0.0"));
    });
}

#[test]
fn offline_install_restores_from_cache_after_vendors_removed() {
    with_isolated_home(|_| {
        let package_dir = TempDir::new().expect("package dir");
        write_package_fixture(package_dir.path(), "smoke.root", "0.1.0");

        let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
        mock.insert_package(Package {
            name: "smoke.dep".to_string(),
            version: "2.0.0".to_string(),
            description: "dep".to_string(),
            ..Package::default()
        });

        let mut pm =
            PackageManager::open_with_registries(package_dir.path(), mock_registries(mock), nyar_package_manager::ProjectLayout::neutral())
                .expect("open package manager");
        pm.install_one("smoke.dep", "2.0.0", "npm", true).expect("online install");

        let vendor_path = package_dir.path().join("vendors");
        std::fs::remove_dir_all(&vendor_path).expect("remove vendors");

        pm.offline = true;
        let info = pm.install_one("smoke.dep", "2.0.0", "npm", false).expect("offline install");
        assert_eq!(info.name, "smoke.dep");
        assert_eq!(info.version, "2.0.0");
        assert!(package_dir.path().join("vendors").join("npm").join("smoke.dep@2.0.0").is_dir());
    });
}

#[test]
fn offline_install_fails_when_package_missing() {
    with_isolated_home(|_| {
        let package_dir = TempDir::new().expect("package dir");
        std::fs::write(
            package_dir.path().join("package.von"),
            "{\n    name: \"smoke.root\",\n    version: \"0.1.0\",\n    dependencies: {\n        \"missing.dep\": \"^1.0.0\",\n    },\n}\n",
        )
        .expect("write manifest");

        let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
        let mut pm =
            PackageManager::open_with_registries(package_dir.path(), mock_registries(mock), nyar_package_manager::ProjectLayout::neutral())
                .expect("open package manager");
        pm.offline = true;
        let error = pm.install_dependencies(false, "npm").expect_err("offline install should fail");
        assert!(error.to_string().contains("offline install failed"));
    });
}

#[test]
fn manifest_roundtrips_peer_dependencies() {
    let manifest = nyar_package_manager::PackageManifest::parse(
        "{\n    name: \"demo\",\n    version: \"1.0.0\",\n    peerDependencies: {\n        \"react\": \"^18.0.0\",\n    },\n}\n",
    )
    .expect("parse manifest");
    assert_eq!(manifest.peer_dependencies.get("react").and_then(|spec| spec.version_constraint()), Some("^18.0.0"));
}

#[test]
fn peer_dependency_validation_warns_on_missing_peer() {
    with_isolated_home(|_| {
        let package_dir = TempDir::new().expect("package dir");
        std::fs::write(
            package_dir.path().join("package.von"),
            "{\n    name: \"smoke.root\",\n    version: \"0.1.0\",\n    peerDependencies: {\n        \"peer.pkg\": \"^1.0.0\",\n    },\n}\n",
        )
        .expect("write manifest");

        let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
        let mut pm =
            PackageManager::open_with_registries(package_dir.path(), mock_registries(mock), nyar_package_manager::ProjectLayout::neutral())
                .expect("open package manager");
        pm.install_dependencies(false, "npm").expect("install with peer warning");
    });
}

#[test]
fn audit_offline_checks_licenses_from_lockfile() {
    with_isolated_home(|_| {
        let package_dir = TempDir::new().expect("package dir");
        write_package_fixture(package_dir.path(), "smoke.root", "0.1.0");

        let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
        mock.insert_package(Package {
            name: "smoke.dep".to_string(),
            version: "2.0.0".to_string(),
            license: "GPL-3.0-only".to_string(),
            ..Package::default()
        });

        let mut pm =
            PackageManager::open_with_registries(package_dir.path(), mock_registries(mock), nyar_package_manager::ProjectLayout::neutral())
                .expect("open package manager");
        pm.install_one("smoke.dep", "2.0.0", "npm", true).expect("install");

        let result = pm.audit(true).expect("audit");
        assert!(result.vulnerabilities.is_empty());
        assert!(result.has_license_issues());
        assert!(result.licenses.iter().any(|license| license.is_restricted));
        assert!(SecurityAudit::is_license_compatible("MIT"));
        assert!(SecurityAudit::is_license_restricted("GPL-3.0-only"));
    });
}

#[test]
fn registry_sources_persist_custom_endpoint() {
    with_isolated_home(|_| {
        let layout = nyar_package_manager::ProjectLayout::neutral();
        let mut manager = RegistrySourceManager::open_with_layout(layout).expect("open");
        manager.add("npm", "https://npm.example.test").expect("add");
        assert_eq!(manager.get_endpoint("npm"), Some("https://npm.example.test"));

        let reloaded = RegistrySourceManager::open_with_layout(layout).expect("reload");
        assert_eq!(reloaded.get_endpoint("npm"), Some("https://npm.example.test"));
        assert!(reloaded.get_endpoint("valhalla").is_some());

        let registries = reloaded.build_registries().expect("build registries");
        assert_eq!(registries.get("npm").expect("npm").endpoint(), "https://npm.example.test");
        assert!(registries.contains_key("valhalla"));
    });
}

#[test]
fn nuget_publish_requires_official_tool_jsr_requires_token() {
    use nyar_package_registry::{JsrRegistry, NugetRegistry, PublishOptions, Registry};

    let nuget = NugetRegistry::new("https://api.nuget.org/v3").expect("nuget");
    let result = nuget
        .publish_package(
            &PublishOptions { package_name: "Demo.Package".to_string(), version: "1.0.0".to_string(), ..PublishOptions::default() },
            b"nupkg",
        )
        .expect("nuget publish result");
    assert!(!result.success);
    assert!(result.official_tool_required);
    assert!(result.message.contains("dotnet nuget push"));

    let jsr = JsrRegistry::new("https://jsr.io").expect("jsr");
    let result = jsr
        .publish_package(
            &PublishOptions { package_name: "@scope/pkg".to_string(), version: "1.0.0".to_string(), ..PublishOptions::default() },
            b"tgz",
        )
        .expect("jsr publish result");
    assert!(!result.success);
    assert!(!result.official_tool_required);
    assert!(result.message.contains("JSR token"));
}

#[test]
fn discovers_npm_token_from_npmrc() {
    use nyar_package_registry::discover_token;

    with_isolated_home(|home| {
        let npmrc = home.join(".npmrc");
        std::fs::write(&npmrc, "//registry.npmjs.org/:_authToken=npm_from_npmrc\n").expect("write npmrc");
        // Temporarily point HOME/USERPROFILE; NYAR_HOME alone is not enough for npmrc;
        // registry credentials use USERPROFILE/HOME. Override both under the lock.
        let previous_home = std::env::var_os("HOME");
        let previous_profile = std::env::var_os("USERPROFILE");
        let previous_npm = std::env::var_os("NPM_TOKEN");
        let previous_node = std::env::var_os("NODE_AUTH_TOKEN");
        unsafe {
            std::env::set_var("HOME", home);
            std::env::set_var("USERPROFILE", home);
            std::env::remove_var("NPM_TOKEN");
            std::env::remove_var("NODE_AUTH_TOKEN");
        }

        let credential = discover_token("npm", Some("https://registry.npmjs.org"), None).expect("discover");
        assert_eq!(credential.token, "npm_from_npmrc");
        assert!(credential.source.contains(".npmrc"));

        unsafe {
            match previous_home {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
            match previous_profile {
                Some(value) => std::env::set_var("USERPROFILE", value),
                None => std::env::remove_var("USERPROFILE"),
            }
            match previous_npm {
                Some(value) => std::env::set_var("NPM_TOKEN", value),
                None => std::env::remove_var("NPM_TOKEN"),
            }
            match previous_node {
                Some(value) => std::env::set_var("NODE_AUTH_TOKEN", value),
                None => std::env::remove_var("NODE_AUTH_TOKEN"),
            }
        }
    });
}

#[test]
fn login_discovers_npmrc_without_auth_von() {
    with_isolated_home(|home| {
        let package_dir = TempDir::new().expect("package dir");
        write_package_fixture(package_dir.path(), "login.smoke", "0.1.0");
        std::fs::write(home.join(".npmrc"), "//registry.npmjs.org/:_authToken=discovered-token\n").expect("npmrc");

        let previous_home = std::env::var_os("HOME");
        let previous_profile = std::env::var_os("USERPROFILE");
        unsafe {
            std::env::set_var("HOME", home);
            std::env::set_var("USERPROFILE", home);
        }

        let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
        mock.insert_token("discovered-token", "npm-user");
        let mut pm =
            PackageManager::open_with_registries(package_dir.path(), mock_registries(mock), nyar_package_manager::ProjectLayout::neutral())
                .expect("open package manager");

        let result = pm.login(Some("npm"), None).expect("login");
        assert!(result.verify.valid);
        assert_eq!(result.verify.username.as_deref(), Some("npm-user"));
        assert!(result.credential_source.contains(".npmrc"));
        assert!(!result.stored_in_auth_von);

        let auth_path = home.join(".nyar").join("auth.von");
        if auth_path.is_file() {
            let auth = std::fs::read_to_string(auth_path).expect("read auth");
            assert!(!auth.contains("discovered-token"));
        }

        unsafe {
            match previous_home {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
            match previous_profile {
                Some(value) => std::env::set_var("USERPROFILE", value),
                None => std::env::remove_var("USERPROFILE"),
            }
        }
    });
}

#[test]
fn valhalla_login_persists_auth_von() {
    use nyar_package_registry::{PublisherKey, Registry, ValhallaRegistry};

    with_isolated_home(|home| {
        let package_dir = TempDir::new().expect("package dir");
        write_package_fixture(package_dir.path(), "valhalla.login", "0.1.0");

        let mut registries: HashMap<String, Arc<dyn Registry>> = HashMap::new();
        registries.insert("valhalla".to_string(), Arc::new(ValhallaRegistry::new("https://valhalla.example.test").expect("valhalla")));
        let mut pm = PackageManager::open_with_registries(package_dir.path(), registries, nyar_package_manager::ProjectLayout::neutral())
            .expect("open package manager");
        let key = PublisherKey::from_seed([7u8; 32]).expect("seed");
        let result = pm.login(Some("valhalla"), Some(&key.to_auth_material())).expect("login");
        assert!(result.verify.valid);
        assert!(result.stored_in_auth_von);

        let auth_path = home.join(".nyar").join("auth.von");
        assert!(auth_path.is_file(), "auth.von should be written for valhalla");
        let auth = std::fs::read_to_string(auth_path).expect("read auth");
        assert!(auth.contains("valhalla"));
    });
}

#[test]
fn publish_discovers_nuget_token_from_config() {
    use nyar_package_registry::discover_token;

    with_isolated_home(|home| {
        let nuget_dir = home.join("NuGet");
        std::fs::create_dir_all(&nuget_dir).expect("mkdir");
        std::fs::write(
            nuget_dir.join("NuGet.Config"),
            r#"<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <apikeys>
    <add key="https://api.nuget.org/v3/index.json" value="nuget-publish-token" />
  </apikeys>
</configuration>
"#,
        )
        .expect("write config");

        let previous = std::env::var_os("APPDATA");
        let previous_home = std::env::var_os("HOME");
        let previous_profile = std::env::var_os("USERPROFILE");
        unsafe {
            std::env::set_var("APPDATA", home);
            std::env::set_var("HOME", home);
            std::env::set_var("USERPROFILE", home);
        }

        let credential = discover_token("nuget", Some("https://api.nuget.org/v3"), None).expect("discover");
        assert_eq!(credential.token, "nuget-publish-token");

        unsafe {
            match previous {
                Some(value) => std::env::set_var("APPDATA", value),
                None => std::env::remove_var("APPDATA"),
            }
            match previous_home {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
            match previous_profile {
                Some(value) => std::env::set_var("USERPROFILE", value),
                None => std::env::remove_var("USERPROFILE"),
            }
        }
    });
}

#[test]
fn valhalla_publish_requires_signer_key() {
    use nyar_package_registry::{PublishOptions, PublisherKey, Registry, RegistryError, ValhallaRegistry};

    let registry = ValhallaRegistry::new("https://valhalla.example.test").expect("valhalla");
    let missing = registry
        .publish_package(
            &PublishOptions { package_name: "Demo_Pkg".to_string(), version: "0.1.0".to_string(), ..PublishOptions::default() },
            b"blob",
        )
        .expect_err("valhalla publish should require key");
    assert!(matches!(missing, RegistryError::Message(_)));

    let seed = [9u8; 32];
    let key = PublisherKey::from_seed(seed).expect("seed");
    let fingerprint_only = PublisherKey::parse(key.fingerprint()).expect("fingerprint");
    assert!(!fingerprint_only.can_sign());

    let result = registry.verify_token(&key.to_auth_material()).expect("verify");
    assert!(result.valid);
    assert_eq!(result.username.as_deref(), Some(key.fingerprint()));

    // Fingerprint-only key is accepted for identity but cannot publish.
    let finger_only_err = registry
        .publish_package(
            &PublishOptions {
                package_name: "demo.pkg".to_string(),
                version: "0.1.0".to_string(),
                auth_token: Some(key.fingerprint().to_string()),
                ..PublishOptions::default()
            },
            b"blob",
        )
        .expect_err("fingerprint-only cannot sign");
    assert!(finger_only_err.to_string().contains("fingerprint-only"));
}

#[test]
fn nested_workspace_enumerates_member_packages() {
    let root = TempDir::new().expect("workspace root");
    let nested_dir = root.path().join("nested");
    let app_dir = nested_dir.join("app");
    std::fs::create_dir_all(&app_dir).expect("app dir");
    let layout = nyar_package_manager::ProjectLayout::neutral();
    std::fs::write(root.path().join(layout.workspace_manifest), "{\n    name: \"root\",\n    members: [\"nested\"]\n}\n")
        .expect("root workspace");
    std::fs::write(nested_dir.join(layout.workspace_manifest), "{\n    name: \"nested\",\n    members: [\"app\"]\n}\n")
        .expect("nested workspace");
    std::fs::write(app_dir.join(layout.package_manifest), "{\n    name: \"nested.app\",\n    version: \"1.0.0\"\n}\n").expect("app package");

    let workspace = nyar_package_manager::WorkspaceManifest::load(root.path(), layout).expect("load workspace");
    let members = workspace.enumerate_member_dirs(root.path(), layout).expect("enumerate members");
    assert_eq!(members.len(), 1);
    assert_eq!(members[0], app_dir);

    let packages = workspace.member_packages(root.path(), layout).expect("member packages");
    assert_eq!(packages.get("nested.app").map(|path| path.as_path()), Some(app_dir.as_path()));
    assert_eq!(packages.get("app").map(|path| path.as_path()), Some(app_dir.as_path()));
}

#[test]
fn open_with_manifest_layout_uses_product_lockfile_name() {
    with_isolated_home(|_| {
        let package_dir = TempDir::new().expect("package dir");
        let layout = nyar_package_manager::ProjectLayout {
            lockfile: "product-lock.von",
            home_dirname: ".product-pm",
            home_env: "PRODUCT_PM_HOME",
            ..nyar_package_manager::ProjectLayout::neutral()
        };

        let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
        mock.insert_package(Package {
            name: "layout.dep".to_string(),
            version: "1.0.0".to_string(),
            description: "dep".to_string(),
            ..Package::default()
        });

        let manifest =
            nyar_package_manager::PackageManifest::parse("{\n    name: \"layout.root\",\n    version: \"0.1.0\",\n}\n").expect("parse");
        let mut pm =
            PackageManager::open_with_manifest_layout(package_dir.path(), manifest, mock_registries(mock), layout).expect("open with layout");
        pm.install_one("layout.dep", "1.0.0", "npm", false).expect("install");

        assert!(package_dir.path().join("product-lock.von").is_file());
        assert!(!package_dir.path().join("package-lock.von").exists());
    });
}

#[test]
fn update_preserves_dev_dependency_bucket() {
    with_isolated_home(|_| {
        let package_dir = TempDir::new().expect("package dir");
        std::fs::write(
            package_dir.path().join("package.von"),
            "{\n    name: \"smoke.root\",\n    version: \"0.1.0\",\n    dev_dependencies: {\n        \"smoke.dev\": \"1.0.0\",\n    },\n}\n",
        )
        .expect("write manifest");

        let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
        mock.insert_package(Package {
            name: "smoke.dev".to_string(),
            version: "1.0.0".to_string(),
            description: "dev".to_string(),
            ..Package::default()
        });
        mock.insert_package(Package {
            name: "smoke.dev".to_string(),
            version: "2.0.0".to_string(),
            description: "dev".to_string(),
            ..Package::default()
        });

        let mut pm =
            PackageManager::open_with_registries(package_dir.path(), mock_registries(mock), nyar_package_manager::ProjectLayout::neutral())
                .expect("open");
        pm.update_one_with_manifest("smoke.dev", "2.0.0", "npm", true).expect("update");

        let saved = std::fs::read_to_string(package_dir.path().join("package.von")).expect("read manifest");
        assert!(saved.contains("dev_dependencies"), "dev bucket lost: {saved}");
        assert!(saved.contains("2.0.0"), "version not updated: {saved}");
        // Must not migrate into runtime dependencies.
        let parsed = nyar_package_manager::PackageManifest::parse(&saved).expect("parse");
        assert!(!parsed.dependencies.contains_key("smoke.dev"));
        assert_eq!(parsed.dev_dependencies.get("smoke.dev").and_then(|s| s.version_constraint()), Some("2.0.0"));
    });
}

#[test]
fn remove_cleans_registry_vendor_path() {
    with_isolated_home(|_| {
        let package_dir = TempDir::new().expect("package dir");
        write_package_fixture(package_dir.path(), "smoke.root", "0.1.0");

        let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
        mock.insert_package(Package {
            name: "smoke.dep".to_string(),
            version: "2.0.0".to_string(),
            description: "dep".to_string(),
            ..Package::default()
        });

        let mut pm =
            PackageManager::open_with_registries(package_dir.path(), mock_registries(mock), nyar_package_manager::ProjectLayout::neutral())
                .expect("open");
        pm.install_one("smoke.dep", "2.0.0", "npm", true).expect("install");

        let vendor_path = package_dir.path().join("vendors").join("npm").join("smoke.dep@2.0.0");
        assert!(vendor_path.is_dir());

        pm.remove_dependency("smoke.dep").expect("remove");
        assert!(!vendor_path.exists(), "registry vendor path should be removed");
        assert!(!package_dir.path().join("vendors").join("smoke.dep").exists());
    });
}

#[test]
fn add_dev_dependency_writes_dev_bucket() {
    with_isolated_home(|_| {
        let package_dir = TempDir::new().expect("package dir");
        write_package_fixture(package_dir.path(), "smoke.root", "0.1.0");

        let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
        mock.insert_package(Package {
            name: "smoke.dev".to_string(),
            version: "1.5.0".to_string(),
            description: "dev".to_string(),
            ..Package::default()
        });

        let mut pm =
            PackageManager::open_with_registries(package_dir.path(), mock_registries(mock), nyar_package_manager::ProjectLayout::neutral())
                .expect("open");
        pm.add_dev_dependency("smoke.dev", "1.5.0", "npm").expect("add dev");

        let parsed =
            nyar_package_manager::PackageManifest::load(package_dir.path(), nyar_package_manager::ProjectLayout::neutral()).expect("load");
        assert!(!parsed.dependencies.contains_key("smoke.dev"));
        assert_eq!(parsed.dev_dependencies.get("smoke.dev").and_then(|s| s.version_constraint()), Some("1.5.0"));
    });
}
