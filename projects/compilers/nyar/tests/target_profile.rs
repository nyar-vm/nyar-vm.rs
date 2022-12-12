use nyar::{
    CanonicalAbi, HostProjectionBoundary, PublishFormat, ReferenceManagement, RunnerFamily, RunnerSelector, TargetHostKind,
    abstractions::CanonicalTarget,
};

#[test]
fn derives_profile_for_clr() {
    let profile = CanonicalTarget::clr().to_profile(None);
    assert_eq!(profile.host_kind, TargetHostKind::DotNet);
    assert_eq!(profile.host_boundary, HostProjectionBoundary::Clr);
    assert_eq!(profile.reference_management, ReferenceManagement::HostGc);
    assert_eq!(profile.runner_family(), RunnerFamily::Clr);
    assert_eq!(profile.artifact_policy.default_publish_format, PublishFormat::Directory);
}

#[test]
fn parses_runner_selector_from_family_and_canonical_target() {
    assert_eq!("clr".parse::<RunnerSelector>().unwrap(), RunnerSelector::Family(RunnerFamily::Clr));
    assert_eq!("wasm32-unknown-browser-wasm".parse::<RunnerSelector>().unwrap(), RunnerSelector::Canonical(CanonicalTarget::wasm()));
}

#[test]
fn derives_profile_for_wasip3() {
    let profile = CanonicalTarget::wasip3().to_profile(None);
    assert_eq!(profile.host_kind, TargetHostKind::Wasi);
    assert_eq!(profile.host_boundary, HostProjectionBoundary::WasiComponent);
    assert_eq!(profile.host_flavor, "wasi-component-model-p3");
    assert_eq!(profile.abi, CanonicalAbi::WasiP3);
    assert!(profile.capability_tags.iter().any(|tag| tag == "wasip3"));
}

#[test]
fn parses_wasip3_alias() {
    let target = CanonicalTarget::parse("wasip3").expect("parse wasip3");
    assert_eq!(target, CanonicalTarget::wasip3());
    assert_eq!(target.to_string(), "wasm32-unknown-wasi-wasip3");
}
