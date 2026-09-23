use nyar::{
    CanonicalTarget, TargetBackendFamily,
    abstractions::BackendInputKind,
    backends::gpu::{GpuDxilBackend, GpuSpirvBackend},
    builtin_graphic_manifest, builtin_neural_manifest,
    packaging::TargetLane,
};
use nyar_emitter::bundled_backend_registry;
use nyar_language::{
    Identifier as ValkyrieIdentifier, NamePath,
    valkyrie::{
        frontend_contract::planning::hir_module_to_frontend_neutral_plan,
        types::hir::{HirBlock, HirDocumentation, HirFunction, HirModule, HirVisibility, ValkyrieType},
    },
};
use nyar_optimizer::{
    FutamuraProjectionFamily, HostProjectionBoundary, OptimizationRequest, OptimizationSession, ProjectionPolicy, ReferenceManagement,
};
use nyar_types::SourceSpan;

#[test]
fn rewrite_theory_manifest_graphic_matches_expected_rule_count() {
    let manifest = builtin_graphic_manifest();
    let theory = manifest.to_rewrite_theory();
    assert_eq!(manifest.rules.len(), 8);
    assert_eq!(manifest.fragment, "graphic");
    assert!(!theory.rules.is_empty());
    assert!(!theory.equations.is_empty());
}

#[test]
fn rewrite_theory_manifest_neural_matches_expected_rule_count() {
    let manifest = builtin_neural_manifest();
    let theory = manifest.to_rewrite_theory();
    assert_eq!(manifest.rules.len(), 5);
    assert_eq!(manifest.fragment, "neural");
    assert!(!theory.rules.is_empty());
}

#[test]
fn gpu_fragment_planning_produces_graphic_semantic_fragment() {
    let module = shader_dot_module();
    let plan = hir_module_to_frontend_neutral_plan(&module);
    let fragment = plan.semantic_fragments.iter().find(|fragment| fragment.id.as_str() == "graphic").expect("graphic fragment");
    assert!(fragment.required_capabilities.iter().any(|cap| cap.as_str() == "gpu-shader"));
    assert!(!fragment.rewrite_theory.rules.is_empty());
    assert!(
        fragment
            .rewrite_theory
            .equations
            .iter()
            .any(|eq| { eq.left.parts().len() == 2 && eq.left.parts()[0].as_str() == "graphic" && eq.left.parts()[1].as_str() == "dot" })
    );
}

#[test]
fn gpu_backend_placeholders_accept_spirv_and_dxil() {
    assert!(GpuSpirvBackend::accept(BackendInputKind::SpirvModule, TargetLane::Gpu));
    assert!(GpuDxilBackend::accept(BackendInputKind::DxilContainer, TargetLane::Gpu));
    assert!(!GpuSpirvBackend::accept(BackendInputKind::DxilContainer, TargetLane::Gpu));
}

#[test]
fn gpu_bundled_registry_registers_spirv_and_dxil_backends() {
    let module = shader_dot_module();
    let plan = hir_module_to_frontend_neutral_plan(&module);
    let mut profile = CanonicalTarget::clr().to_profile(None);
    profile.backend_family = TargetBackendFamily::Gpu;
    let projection = ProjectionPolicy {
        family: FutamuraProjectionFamily::Gpu,
        host_boundary: HostProjectionBoundary::Native,
        reference_management: ReferenceManagement::PerceusRc,
        prefer_small_artifacts: false,
        preserve_effect_boundaries: true,
    };
    let registry = bundled_backend_registry(&plan.semantic_fragments, &profile, &projection);
    assert!(registry.registrations.iter().any(|reg| reg.backend_name == "gpu-spirv"));
    assert!(registry.registrations.iter().any(|reg| reg.backend_name == "gpu-dxil"));
}

#[test]
fn graphic_fragment_optimization_session_applies_equations() {
    let module = shader_dot_module();
    let plan = hir_module_to_frontend_neutral_plan(&module);
    let fragment = plan.semantic_fragments.iter().find(|fragment| fragment.id.as_str() == "graphic").expect("graphic fragment");
    let session = OptimizationSession;
    let result = session.optimize(OptimizationRequest {
        program: plan.object_algebraic_program.clone(),
        rewrite_theory: fragment.rewrite_theory.clone(),
        capabilities: fragment.required_capabilities.clone(),
        projection_policy: ProjectionPolicy {
            family: FutamuraProjectionFamily::Gpu,
            host_boundary: HostProjectionBoundary::Native,
            reference_management: ReferenceManagement::PerceusRc,
            prefer_small_artifacts: false,
            preserve_effect_boundaries: true,
        },
    });
    assert!(result.egraph.applied_equation_count >= 1);
    assert!(!result.applied_rules.is_empty());
}

fn shader_dot_module() -> HirModule {
    let mut module = HirModule::default();
    module.name = NamePath::new(vec![ValkyrieIdentifier::new("demo")]);
    module.functions.push(HirFunction {
        name: ValkyrieIdentifier::new("main_vs"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: Vec::new(),
        return_type: ValkyrieType::Unit,
        body: HirBlock { statements: Vec::new(), expr: None, span: SourceSpan::new(Default::default(), 0, 0) },
        span: SourceSpan::new(Default::default(), 0, 0),
        visibility: HirVisibility::public(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    });
    module
}
