//! Migrated from `src/deploy.rs` (`tests`).

use super::*;

#[test]
fn parses_cdn_serverless_profile() {
    let source = r#"{
  name: "cdn+serverless",
  description: "SSG to CDN + optional Atlas server islands",
  status: "implemented",
  topology: {
static: "cdn",
server: "serverless"
  },
  artifacts: [
{
  pipeline: "asgard",
  platform: "browser",
  role: "static-web",
  output: "apps/web/dist",
  status: "implemented"
},
{
  pipeline: "atlas",
  role: "server-islands",
  output: "apps/api/dist",
  status: "planned"
}
  ]
}"#;
    let plan = DeployProfile::parse(source).expect("parse");
    assert_eq!(plan.profile.name, "cdn+serverless");
    assert_eq!(plan.profile.artifacts.len(), 2);
    assert_eq!(plan.profile.topology.static_host.as_deref(), Some("cdn"));
    assert!(plan.warnings.is_empty());
}

#[test]
fn rejects_empty_artifacts() {
    let source = r#"{ name: "x", artifacts: [] }"#;
    assert!(DeployProfile::parse(source).is_err());
}

#[test]
fn loads_repo_cdn_profile_file() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../valkyrie.v/projects/asgard._/projects/asgard/deploy/profiles/cdn+serverless.von");
    let plan = DeployProfile::load(&path).expect("load repo profile");
    assert_eq!(plan.profile.name, "cdn+serverless");
    assert_eq!(plan.profile.artifacts.len(), 2);
}

#[test]
fn loads_fullstack_cdn_profile() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../valkyrie.v/examples/test.fullstack/deploy/profiles/cdn+serverless.von");
    let plan = DeployProfile::load(&path).expect("load fullstack profile");
    assert_eq!(plan.profile.name, "cdn+serverless");
    assert!(plan.profile.artifacts.iter().any(|a| a.output.contains("apps/shell")));
    assert!(plan.profile.artifacts.iter().any(|a| a.output.contains("apps/atlas")));
}

#[test]
fn lists_fullstack_profiles_dir() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../valkyrie.v/examples/test.fullstack/deploy/profiles");
    let names = list_deploy_profiles(&dir).expect("list");
    assert!(names.iter().any(|n| n == "cdn+serverless.von"));
    assert!(names.iter().any(|n| n == "portable.von"));
}
