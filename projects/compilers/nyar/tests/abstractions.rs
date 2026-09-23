use nyar::abstractions::{
    BinaryArch, BinaryFlavor, CanonicalAbi, CanonicalArch, CanonicalSpecification, CanonicalTarget, CanonicalVendor, TargetFamily,
};

#[test]
fn parses_short_aliases() {
    assert_eq!(CanonicalTarget::parse("clr").unwrap().to_string(), "clr-microsoft-unknown-managed");
    assert_eq!(CanonicalTarget::parse("jvm").unwrap().to_string(), "jvm-openjdk-unknown-managed");
    assert_eq!(CanonicalTarget::parse("wasm").unwrap().to_string(), "wasm32-unknown-browser-wasm");
}

#[test]
fn parses_full_canonical_targets() {
    let target = CanonicalTarget::parse("x86_64-pc-windows-msvc").unwrap();
    assert_eq!(target.arch, CanonicalArch::X86_64);
    assert_eq!(target.vendor, CanonicalVendor::Pc);
    assert_eq!(target.specification, CanonicalSpecification::Windows);
    assert_eq!(target.abi, Some(CanonicalAbi::Msvc));
}

#[test]
#[cfg(target_os = "windows")]
fn native_alias_resolves_to_host_canonical_target() {
    let target = CanonicalTarget::parse("native").unwrap();
    assert_eq!(target.to_string(), CanonicalTarget::parse("x86_64-pc-windows-msvc").unwrap().to_string());
}

#[test]
fn projects_to_binary_target() {
    let binary = CanonicalTarget::parse("clr").unwrap().to_binary_target();
    assert_eq!(binary.family, TargetFamily::Clr);
    assert_eq!(binary.arch, BinaryArch::Any);
    assert_eq!(binary.flavor, BinaryFlavor::ManagedClr);
}
