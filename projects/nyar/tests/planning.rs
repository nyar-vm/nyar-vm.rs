use nyar::{
    ArtifactPartitionPlan, BackendCapability, BackendInputKind, BackendInterpreterRegistration, BackendRegistry, CanonicalTarget,
    ClrSuspendStrategy, FutamuraProjectionFamily, HostProjectionBoundary, Identifier, ObjectAlgebraicDimension, ObjectAlgebraicProgram,
    PlanningInput, ProgramFacts, ProjectionPolicy, QualifiedName, ReferenceManagement, RewriteEquation, RewritePhase, RewriteRule,
    RewriteTheory, RuntimeRequirement, SemanticFragment, TargetLane,
};
use nyar_types::{CapabilityTag, ExternalImportLink};

fn qualified_name(parts: &[&str]) -> QualifiedName {
    QualifiedName::new(parts.iter().map(|part| Identifier::new(part)).collect())
}

fn backend_registry_for(target: CanonicalTarget, projection_family: FutamuraProjectionFamily, fragment_names: &[&str]) -> BackendRegistry {
    let mut registry = BackendRegistry::default();
    let binary_target: nyar::BinaryTarget = target.into();
    let (backend_name, interpreter, lane, input_kind) = match projection_family {
        FutamuraProjectionFamily::Clr => ("clr-binary", "clr.msil", TargetLane::Clr, Some(BackendInputKind::MsilText)),
        FutamuraProjectionFamily::Jvm => ("jvm-binary", "jvm.classfile", TargetLane::Jvm, Some(BackendInputKind::JvmClassFile)),
        FutamuraProjectionFamily::Wasm => ("wasm-binary", "wasm.module", TargetLane::Wasm, Some(BackendInputKind::WasmModule)),
        FutamuraProjectionFamily::Native => ("native-binary", "native.object", TargetLane::Native, Some(BackendInputKind::CoffObject)),
        FutamuraProjectionFamily::NyarVm => ("nyar-vm", "nyar.vm", TargetLane::Vm, None),
        FutamuraProjectionFamily::Gpu => ("gpu-spirv", "gpu.spirv", TargetLane::Gpu, Some(BackendInputKind::SpirvModule)),
    };
    for fragment in fragment_names {
        registry.register(BackendInterpreterRegistration {
            backend_name: backend_name.to_string(),
            priority: 100,
            capability: BackendCapability {
                interpreter: Identifier::new(interpreter),
                fragment: Identifier::new(*fragment),
                lane,
                input_kind,
                supported_projection_families: vec![projection_family],
                supported_host_boundaries: Vec::new(),
                supported_targets: vec![binary_target.clone()],
                required_capabilities: Vec::new(),
                reference_management: None,
            },
        });
    }
    registry
}

#[test]
fn planning_runs_optimizer_before_partitioning() {
    let module_name = qualified_name(&["demo"]);
    let program_facts = ProgramFacts {
        module_name: module_name.clone(),
        entries: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        functions: Vec::new(),
        type_definitions: Vec::new(),
        capabilities: vec![CapabilityTag::new("suspend")],
        reference_management: Some(ReferenceManagement::HostGc),
        runtime_requirements: vec![RuntimeRequirement { key: "suspend".to_string(), value: "required".to_string() }],
    };
    let object_algebraic_program = ObjectAlgebraicProgram {
        module_name: module_name.clone(),
        exports: vec![qualified_name(&["demo", "main"])],
        dimensions: vec![ObjectAlgebraicDimension {
            name: Identifier::new("functions"),
            exported_operations: vec![qualified_name(&["demo", "main"])],
            required_capabilities: Vec::new(),
            reference_management_hint: Some(ReferenceManagement::HostGc),
        }],
        structured_terms: Vec::new(),
    };
    let mut rewrite_theory = RewriteTheory::default();
    rewrite_theory.register(RewriteRule {
        name: Identifier::new("pre-projection.suspend-boundary"),
        phase: RewritePhase::PreProjection,
        required_capabilities: vec![CapabilityTag::new("suspend")],
        allowed_projection_families: vec![FutamuraProjectionFamily::Clr],
    });

    let plan = ArtifactPartitionPlan::from_input(PlanningInput {
        module_name,
        target: CanonicalTarget::clr(),
        program_facts,
        semantic_fragments: Vec::new(),
        object_algebraic_program,
        rewrite_theory,
        projection_policy: ProjectionPolicy {
            family: FutamuraProjectionFamily::Clr,
            host_boundary: HostProjectionBoundary::Clr,
            reference_management: ReferenceManagement::HostGc,
            prefer_small_artifacts: false,
            preserve_effect_boundaries: true,
        },
        backend_registry: backend_registry_for(CanonicalTarget::clr(), FutamuraProjectionFamily::Clr, &["functions"]),
        clr_suspend_strategy: ClrSuspendStrategy::default(),
    })
    .expect("plan");

    assert_eq!(plan.optimization.projection.family, FutamuraProjectionFamily::Clr);
    assert_eq!(plan.optimization.projection.host_boundary, HostProjectionBoundary::Clr);
    assert_eq!(plan.optimization.projection.reference_management, ReferenceManagement::HostGc);
    assert_eq!(plan.partitions[0].lane, TargetLane::Clr);
    assert_eq!(plan.partitions[0].input_kind, Some(BackendInputKind::MsilText));
    assert_eq!(plan.partitions[0].host_boundary, HostProjectionBoundary::Clr);
    assert_eq!(plan.partitions[0].reference_management, ReferenceManagement::HostGc);
    assert_eq!(plan.partitions[0].runtime_requirements.len(), 1);
    assert_eq!(plan.optimization.applied_rules[0].as_str(), "pre-projection.suspend-boundary");
}

#[test]
fn planning_can_promote_operation_level_reference_management_hint() {
    let module_name = qualified_name(&["demo"]);
    let operation = qualified_name(&["demo", "main"]);
    let program_facts = ProgramFacts {
        module_name: module_name.clone(),
        entries: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        functions: vec![nyar::FunctionAnalysis {
            symbol: operation.clone(),
            is_external: false,
            can_suspend: false,
            is_async: false,
            uses_host_interop: false,
            external_import_link: None,
            reference_management_hint: Some(ReferenceManagement::HostGc),
            host_provider_for: None,
        }],
        capabilities: Vec::new(),
        reference_management: None,
        runtime_requirements: Vec::new(),
        type_definitions: Vec::new(),
    };
    let object_algebraic_program = ObjectAlgebraicProgram {
        module_name: module_name.clone(),
        exports: vec![operation.clone()],
        dimensions: vec![ObjectAlgebraicDimension {
            name: Identifier::new("functions"),
            exported_operations: vec![operation],
            required_capabilities: Vec::new(),
            reference_management_hint: Some(ReferenceManagement::HostGc),
        }],
        structured_terms: Vec::new(),
    };

    let plan = ArtifactPartitionPlan::from_input(PlanningInput {
        module_name,
        target: CanonicalTarget::clr(),
        program_facts,
        semantic_fragments: Vec::new(),
        object_algebraic_program,
        rewrite_theory: RewriteTheory::default(),
        projection_policy: ProjectionPolicy {
            family: FutamuraProjectionFamily::Clr,
            host_boundary: HostProjectionBoundary::Clr,
            reference_management: ReferenceManagement::PerceusRc,
            prefer_small_artifacts: false,
            preserve_effect_boundaries: true,
        },
        backend_registry: backend_registry_for(CanonicalTarget::clr(), FutamuraProjectionFamily::Clr, &["functions"]),
        clr_suspend_strategy: ClrSuspendStrategy::default(),
    })
    .expect("plan");

    assert_eq!(plan.partitions[0].reference_management, ReferenceManagement::HostGc);
}

#[test]
fn planning_splits_partitions_by_dimension() {
    let module_name = qualified_name(&["demo"]);
    let host_operation = qualified_name(&["demo", "host_call"]);
    let suspend_operation = qualified_name(&["demo", "await_next"]);
    let base_operation = qualified_name(&["demo", "main"]);
    let program_facts = ProgramFacts {
        module_name: module_name.clone(),
        entries: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        functions: vec![
            nyar::FunctionAnalysis {
                symbol: base_operation.clone(),
                is_external: false,
                can_suspend: false,
                is_async: false,
                uses_host_interop: false,
                external_import_link: None,
                host_provider_for: None,
                reference_management_hint: None,
            },
            nyar::FunctionAnalysis {
                symbol: host_operation.clone(),
                is_external: false,
                can_suspend: false,
                is_async: false,
                uses_host_interop: true,
                external_import_link: Some(ExternalImportLink::host(Some(Identifier::new("clr")), vec!["mscorlib".to_string()])),
                host_provider_for: None,
                reference_management_hint: Some(ReferenceManagement::HostGc),
            },
            nyar::FunctionAnalysis {
                symbol: suspend_operation.clone(),
                is_external: false,
                can_suspend: true,
                is_async: true,
                uses_host_interop: false,
                external_import_link: None,
                host_provider_for: None,
                reference_management_hint: Some(ReferenceManagement::HostGc),
            },
        ],
        capabilities: vec![CapabilityTag::new("host-interop"), CapabilityTag::new("suspend")],
        reference_management: None,
        runtime_requirements: vec![
            RuntimeRequirement { key: "host-interop".to_string(), value: "required".to_string() },
            RuntimeRequirement { key: "suspend".to_string(), value: "required".to_string() },
        ],
        type_definitions: Vec::new(),
    };
    let object_algebraic_program = ObjectAlgebraicProgram {
        module_name: module_name.clone(),
        exports: vec![base_operation.clone(), host_operation.clone(), suspend_operation.clone()],
        dimensions: vec![
            ObjectAlgebraicDimension {
                name: Identifier::new("functions"),
                exported_operations: vec![base_operation],
                required_capabilities: Vec::new(),
                reference_management_hint: None,
            },
            ObjectAlgebraicDimension {
                name: Identifier::new("host-interop"),
                exported_operations: vec![host_operation],
                required_capabilities: vec![CapabilityTag::new("host-interop")],
                reference_management_hint: Some(ReferenceManagement::HostGc),
            },
            ObjectAlgebraicDimension {
                name: Identifier::new("suspend"),
                exported_operations: vec![suspend_operation],
                required_capabilities: vec![CapabilityTag::new("suspend")],
                reference_management_hint: Some(ReferenceManagement::HostGc),
            },
        ],
        structured_terms: Vec::new(),
    };

    let plan = ArtifactPartitionPlan::from_input(PlanningInput {
        module_name,
        target: CanonicalTarget::clr(),
        program_facts,
        semantic_fragments: Vec::new(),
        object_algebraic_program,
        rewrite_theory: RewriteTheory::default(),
        projection_policy: ProjectionPolicy {
            family: FutamuraProjectionFamily::Clr,
            host_boundary: HostProjectionBoundary::Clr,
            reference_management: ReferenceManagement::PerceusRc,
            prefer_small_artifacts: false,
            preserve_effect_boundaries: true,
        },
        backend_registry: backend_registry_for(
            CanonicalTarget::clr(),
            FutamuraProjectionFamily::Clr,
            &["functions", "host-interop", "suspend"],
        ),
        clr_suspend_strategy: ClrSuspendStrategy::default(),
    })
    .expect("plan");

    assert_eq!(plan.partitions.len(), 3);
    assert_eq!(plan.partitions[0].name, "demo::functions");
    assert_eq!(plan.partitions[1].name, "demo::host-interop");
    assert_eq!(plan.partitions[2].name, "demo::suspend");
    assert_eq!(plan.partitions[0].capabilities.len(), 2);
    assert_eq!(plan.partitions[1].capabilities, vec![CapabilityTag::new("host-interop")]);
    assert_eq!(plan.partitions[2].capabilities, vec![CapabilityTag::new("suspend")]);
    assert_eq!(
        plan.partitions[1].runtime_requirements,
        vec![RuntimeRequirement { key: "host-interop".to_string(), value: "required".to_string() }]
    );
    assert_eq!(plan.partitions[2].runtime_requirements, vec![RuntimeRequirement { key: "suspend".to_string(), value: "required".to_string() }]);
    assert_eq!(plan.partitions[1].reference_management, ReferenceManagement::HostGc);
    assert_eq!(plan.partitions[2].reference_management, ReferenceManagement::HostGc);
}

#[test]
fn planning_can_build_program_from_semantic_fragments() {
    let module_name = qualified_name(&["demo"]);
    let main = qualified_name(&["demo", "main"]);
    let host = qualified_name(&["demo", "host_call"]);
    let mut suspend_theory = RewriteTheory::default();
    suspend_theory.register(RewriteRule {
        name: Identifier::new("pre-projection.suspend-boundary"),
        phase: RewritePhase::PreProjection,
        required_capabilities: vec![CapabilityTag::new("suspend")],
        allowed_projection_families: vec![FutamuraProjectionFamily::Clr],
    });
    suspend_theory.equate(RewriteEquation {
        left: main.clone(),
        right: host.clone(),
        phase: RewritePhase::Saturate,
        required_capabilities: vec![CapabilityTag::new("suspend")],
    });

    let plan = ArtifactPartitionPlan::from_input(PlanningInput {
        module_name: module_name.clone(),
        target: CanonicalTarget::clr(),
        program_facts: ProgramFacts {
            module_name: module_name.clone(),
            entries: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            functions: Vec::new(),
            capabilities: vec![CapabilityTag::new("suspend")],
            reference_management: None,
            runtime_requirements: vec![RuntimeRequirement { key: "suspend".to_string(), value: "required".to_string() }],
            type_definitions: Vec::new(),
        },
        semantic_fragments: vec![
            SemanticFragment {
                id: Identifier::new("functions"),
                exported_operations: vec![main],
                required_capabilities: Vec::new(),
                reference_management_hint: None,
                entry_operation: None,
                external_import_links: std::collections::BTreeMap::new(),
                external_call_edges: Vec::new(),
                internal_call_edges: Vec::new(),
                operation_literal_returns: std::collections::BTreeMap::new(),
                operation_void_returns: Default::default(),
                witness_tables: Vec::new(),
                witness_calls: Vec::new(),
                rewrite_theory: RewriteTheory::default(),
            },
            SemanticFragment {
                id: Identifier::new("suspend"),
                exported_operations: vec![host],
                required_capabilities: vec![CapabilityTag::new("suspend")],
                reference_management_hint: Some(ReferenceManagement::HostGc),
                entry_operation: None,
                external_import_links: std::collections::BTreeMap::new(),
                external_call_edges: Vec::new(),
                internal_call_edges: Vec::new(),
                operation_literal_returns: std::collections::BTreeMap::new(),
                operation_void_returns: Default::default(),
                witness_tables: Vec::new(),
                witness_calls: Vec::new(),
                rewrite_theory: suspend_theory,
            },
        ],
        object_algebraic_program: ObjectAlgebraicProgram::default(),
        rewrite_theory: RewriteTheory::default(),
        projection_policy: ProjectionPolicy {
            family: FutamuraProjectionFamily::Clr,
            host_boundary: HostProjectionBoundary::Clr,
            reference_management: ReferenceManagement::HostGc,
            prefer_small_artifacts: false,
            preserve_effect_boundaries: true,
        },
        backend_registry: backend_registry_for(CanonicalTarget::clr(), FutamuraProjectionFamily::Clr, &["functions", "suspend"]),
        clr_suspend_strategy: ClrSuspendStrategy::default(),
    })
    .expect("plan");

    assert_eq!(plan.partitions.len(), 2);
    assert_eq!(plan.partitions[0].name, "demo::functions");
    assert_eq!(plan.partitions[1].name, "demo::suspend");
    assert!(plan.optimization.applied_rules.iter().any(|rule| rule.as_str() == "pre-projection.suspend-boundary"));
    assert_eq!(plan.optimization.egraph.applied_equation_count, 1);
}
