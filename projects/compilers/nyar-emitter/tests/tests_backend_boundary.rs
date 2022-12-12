use nyar::{CapabilityTag, Identifier, RewriteTheory, TargetBackendFamily, TheoryBundle};

use nyar_emitter::{
    BackendBoundaryError, BackendDispatchKind, BackendRoute, FragmentSubmission, bundled_backend_capability_descriptor, infer_dispatch_kind,
    validate_dispatch_for_route,
};

fn empty_submission() -> FragmentSubmission {
    FragmentSubmission {
        module_name: "app".to_string(),
        fragment_id: Identifier::new("functions"),
        exported_operations: Vec::new(),
        required_capabilities: Vec::new(),
        theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
        entry_operation: None,
        external_import_links: Default::default(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        witness_tables: Vec::new(),
        witness_calls: Vec::new(),
        control_flow: None,
        suspend_runtime: None,
        ..Default::default()
    }
}

fn witness_submission() -> FragmentSubmission {
    let mut submission = empty_submission();
    submission.required_capabilities.push(CapabilityTag::new("trait-witness"));
    submission
}

#[test]
fn clr_route_accepts_trait_witness_dispatch() {
    let route = bundled_backend_capability_descriptor(TargetBackendFamily::Clr).unwrap().backend_route;
    assert_eq!(route, BackendRoute::WitnessCapable);
    validate_dispatch_for_route(route, BackendDispatchKind::Witness).expect("clr accepts witness");
}

#[test]
fn jvm_route_accepts_trait_witness_dispatch() {
    let route = bundled_backend_capability_descriptor(TargetBackendFamily::Jvm).unwrap().backend_route;
    assert_eq!(route, BackendRoute::WitnessCapable);
    validate_dispatch_for_route(route, BackendDispatchKind::Witness).expect("jvm accepts witness");
}

#[test]
fn native_route_accepts_trait_witness_dispatch() {
    let route = bundled_backend_capability_descriptor(TargetBackendFamily::Native).unwrap().backend_route;
    assert_eq!(route, BackendRoute::WitnessCapable);
    validate_dispatch_for_route(route, BackendDispatchKind::Witness).expect("native accepts witness");
    assert_eq!(infer_dispatch_kind(&witness_submission()), BackendDispatchKind::Witness);
}

#[test]
fn wasm_route_accepts_trait_witness_dispatch() {
    let route = bundled_backend_capability_descriptor(TargetBackendFamily::Wasm).unwrap().backend_route;
    assert_eq!(route, BackendRoute::WitnessCapable);
    validate_dispatch_for_route(route, BackendDispatchKind::Witness).expect("wasm accepts witness");
}

#[test]
fn wasm_route_rejects_effect_handler_dispatch() {
    let route = bundled_backend_capability_descriptor(TargetBackendFamily::Wasm).unwrap().backend_route;
    let error = validate_dispatch_for_route(route, BackendDispatchKind::EffectHandler).unwrap_err();
    assert_eq!(error, BackendBoundaryError::UnsupportedEffectDispatch { route: BackendRoute::WitnessCapable });
}

#[test]
fn nyar_vm_route_accepts_effect_handler_dispatch() {
    let route = bundled_backend_capability_descriptor(TargetBackendFamily::NyarVm).unwrap().backend_route;
    assert_eq!(route, BackendRoute::Full);
    validate_dispatch_for_route(route, BackendDispatchKind::EffectHandler).expect("nyar-vm accepts effect handler");
}
