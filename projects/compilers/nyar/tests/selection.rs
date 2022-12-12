use nyar::{
    BackendCandidate, BackendInputKind, BackendSelector, BinaryFlavor, BinaryTarget, HostProjectionBoundary, PartitionBackendRequirement,
    ReferenceManagement, TargetFamily, TargetLane,
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

#[test]
fn builds_backend_requirement_from_partition_plan() {
    let mut registry = nyar::BackendRegistry::default();
    registry.register(nyar::BackendInterpreterRegistration {
        backend_name: "clr-binary".to_string(),
        priority: 100,
        capability: nyar::BackendCapability {
            interpreter: nyar::Identifier::new("clr.msil"),
            fragment: nyar::Identifier::new("functions"),
            lane: TargetLane::Clr,
            input_kind: Some(BackendInputKind::MsilText),
            supported_projection_families: vec![nyar::FutamuraProjectionFamily::Clr],
            supported_host_boundaries: vec![nyar::HostProjectionBoundary::Clr],
            supported_targets: vec![nyar::CanonicalTarget::clr().into()],
            required_capabilities: Vec::new(),
            reference_management: Some(ReferenceManagement::HostGc),
        },
    });
    let plan = nyar::ArtifactPartitionPlan::from_input(nyar::PlanningInput {
        module_name: nyar::QualifiedName::new(vec![nyar::Identifier::new("demo")]),
        target: nyar::CanonicalTarget::clr(),
        program_facts: nyar::ProgramFacts::default(),
        semantic_fragments: Vec::new(),
        object_algebraic_program: nyar::ObjectAlgebraicProgram::default(),
        rewrite_theory: nyar::RewriteTheory::default(),
        projection_policy: nyar::ProjectionPolicy {
            family: nyar::FutamuraProjectionFamily::Clr,
            host_boundary: nyar::HostProjectionBoundary::Clr,
            reference_management: nyar::ReferenceManagement::HostGc,
            prefer_small_artifacts: false,
            preserve_effect_boundaries: true,
        },
        backend_registry: registry,
        clr_suspend_strategy: nyar::ClrSuspendStrategy::default(),
    })
    .expect("plan");

    let requirement = plan.backend_requirement(0).expect("backend requirement");
    assert_eq!(requirement.backend_name, "clr-binary");
    assert_eq!(requirement.fragment.as_str(), "functions");
    assert_eq!(requirement.lane, TargetLane::Clr);
    assert_eq!(requirement.input_kind, BackendInputKind::MsilText);
    assert_eq!(requirement.target.family, TargetFamily::Clr);
    assert_eq!(requirement.host_boundary, HostProjectionBoundary::Clr);
    assert_eq!(requirement.reference_management, ReferenceManagement::HostGc);
}
