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
