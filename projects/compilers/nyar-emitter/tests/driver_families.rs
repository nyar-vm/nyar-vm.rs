use std::path::PathBuf;

use nyar::{HostProjectionBoundary, TargetBackendFamily};
use nyar_emitter::{
    FragmentSubmission, bundled_backend_capability_descriptor, driver_compiler_names, lowering::lower_fragment_to_driver_input,
    nyar_backend_wasi::WasmPackageKind,
};

#[test]
fn frozen_families_have_neither_capability_nor_lowering() {
    for (family, boundary) in [
        (TargetBackendFamily::Clr, HostProjectionBoundary::Clr),
        (TargetBackendFamily::Jvm, HostProjectionBoundary::Jvm),
        (TargetBackendFamily::Native, HostProjectionBoundary::Native),
    ] {
        assert!(bundled_backend_capability_descriptor(family).is_none());
        let result =
            lower_fragment_to_driver_input(&FragmentSubmission::default(), family, boundary, PathBuf::new(), "", WasmPackageKind::Binary);
        let error = match result {
            Ok(_) => panic!("冻结目标不得存在正式 lowering"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("拒绝切换目标或补造产物"), "{error}");
    }
    assert!(driver_compiler_names().iter().all(|name| matches!(*name, "wasm-binary" | "ovm")));
}

#[cfg(not(feature = "ovm-lane"))]
#[test]
fn disabled_vm_does_not_advertise_capability() {
    assert!(bundled_backend_capability_descriptor(TargetBackendFamily::Ovm).is_none());
}
