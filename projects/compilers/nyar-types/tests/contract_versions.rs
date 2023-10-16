use nyar_types::{IDENTITY_SCHEMA_VERSION, LAYOUT_PLAN_VERSION, MIR_CONTRACT_VERSION, contract_version_fingerprint};

#[test]
fn contract_version_fingerprint_is_stable() {
    assert_eq!(IDENTITY_SCHEMA_VERSION, 1);
    assert_eq!(MIR_CONTRACT_VERSION, 9);
    assert_eq!(LAYOUT_PLAN_VERSION, 3);
    assert_eq!(contract_version_fingerprint(), "identity=1;mir=9;layout=3");
}
