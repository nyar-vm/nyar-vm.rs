use nyar::{
    BackendCandidate, BackendInputKind, BackendSelector, BinaryFlavor, BinaryTarget, HostProjectionBoundary, PartitionBackendRequirement,
    ReferenceManagement, RewriteTheory, SemanticFragment, TargetFamily, TargetLane,
};

#[test]
fn selector_prefers_lane_input_target_consistent_backend() {
    let target = BinaryTarget::new(TargetFamily::Clr, nyar::BinaryArch::Any, BinaryFlavor::ManagedClr);
    let requirement = PartitionBackendRequirement {
        backend_name: "clr-binary".to_string(),
        interpreter: nyar::Identifier::new("clr.msil"),
        fragment: nyar::Identifier::new("functions"),
        lane: TargetLane::Clr,
        input_kind: BackendInputKind::MsilText,
        target: target.clone(),
        host_boundary: HostProjectionBoundary::Clr,
        reference_management: ReferenceManagement::HostGc,
    };
    let mut selector = BackendSelector::default();
    selector.register(BackendCandidate {
        name: "clr-binary".to_string(),
        requirement: PartitionBackendRequirement {
            backend_name: "clr-binary".to_string(),
            interpreter: nyar::Identifier::new("clr.msil"),
            fragment: nyar::Identifier::new("functions"),
            lane: TargetLane::Clr,
            input_kind: BackendInputKind::MsilText,
            target: target.clone(),
            host_boundary: HostProjectionBoundary::Clr,
            reference_management: ReferenceManagement::HostGc,
        },
        priority: 100,
    });
    selector.register(BackendCandidate {
        name: "wrong-target".to_string(),
        requirement: PartitionBackendRequirement {
            backend_name: "wrong-target".to_string(),
            interpreter: nyar::Identifier::new("clr.msil"),
            fragment: nyar::Identifier::new("functions"),
            lane: TargetLane::Clr,
            input_kind: BackendInputKind::MsilText,
            target: BinaryTarget::new(TargetFamily::Jvm, nyar::BinaryArch::Any, BinaryFlavor::ManagedClr),
            host_boundary: HostProjectionBoundary::Clr,
            reference_management: ReferenceManagement::HostGc,
        },
        priority: 500,
    });

    let selected = selector.select(&requirement).expect("missing candidate");
    assert_eq!(selected.name, "clr-binary");
}

