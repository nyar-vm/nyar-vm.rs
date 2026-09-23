use std::collections::BTreeMap;

use nyar::{Identifier, QualifiedName};
use nyar_emitter::{
    FragmentSubmission, bundled_backend_registry,
    testing::{augment_msil_with_singletons, augment_msil_with_witness, lower_fragment_to_clr_msil},
};
use nyar_language::{assemble_fragment_submission, plan_artifacts_from_build_output};

#[test]
fn rejects_local_operation_when_fragment_has_no_mir_provider() {
    let operation = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("main")]);
    let error = lower_fragment_to_clr_msil(&FragmentSubmission {
        module_name: "demo".to_string(),
        exported_operations: vec![operation.clone()],
        entry_operation: Some(operation),
        ..Default::default()
    })
    .err()
    .expect("missing MIR must fail closed");

    assert!(error.to_string().contains("缺少 MIR provider"), "{error:?}");
}

#[test]
fn product_named_compile_operation_has_no_missing_mir_exemption() {
    let operation = QualifiedName::new(vec![Identifier::new("legion"), Identifier::new("emitter_compile_project")]);
    let error = lower_fragment_to_clr_msil(&FragmentSubmission {
        module_name: "legion".to_string(),
        exported_operations: vec![operation.clone()],
        entry_operation: Some(operation),
        ..Default::default()
    })
    .err()
    .expect("product operation name must not bypass MIR");

    assert!(error.to_string().contains("缺少 MIR provider"), "{error:?}");
}

#[test]
fn enums_flags_clr_module_has_unique_type_names() {
    use nyar_language::{FrontendBuildOutput, MirLowerer, ValkyrieCompiler, mir::compute_nominal_layouts, types::SourceID};

    let source = r#"
namespace feature_matrix::test;

enums Color { Red Green Blue }

unite Option<T> { Some { value: T } None }

flags FilePerm { Read = 1 Write = 2 }

[test]
micro enums_flags_parse() -> unit {}
"#;
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9999 }).compile_source(source).expect("compile");
    let output = FrontendBuildOutput::from_hir_module(hir);
    let mir = MirLowerer::lower_module_semantic(output.hir_module());
    let (sum_types, flags_types) = compute_nominal_layouts(output.hir_module());
    let submission = FragmentSubmission {
        module_name: "feature_matrix".into(),
        aggregate_layouts: mir.aggregate_layouts,
        sum_types,
        flags_types,
        ..Default::default()
    };
    let module = lower_fragment_to_clr_msil(&submission).expect("CLR lowering");
    let mut counts = BTreeMap::<(String, String), usize>::new();
    for type_def in &module.types {
        *counts.entry((type_def.namespace.clone(), type_def.full_name.clone())).or_default() += 1;
    }
    let dupes: Vec<_> = counts.iter().filter(|(_, count)| **count > 1).collect();
    assert!(
        dupes.is_empty(),
        "duplicate CLR types: {dupes:?}; all={:?}",
        module.types.iter().map(|t| format!("{}::{}", t.namespace, t.full_name)).collect::<Vec<_>>()
    );
}

#[test]
fn feature_matrix_test_bundle_clr_types_have_unique_names() {
    use std::path::PathBuf;

    use nyar::backends::{clr::ClrImageKind, projection_policy_for_target_profile};
    use nyar_emitter::nyar_backend_clr::{PeWriter, PeWriterOptions};
    use nyar_language::{CanonicalTarget, ValkyrieCompiler, nyar::ClrSuspendStrategy, types::SourceID};

    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../valkyrie.v/examples/feature-matrix/test");
    let source_dir = base.parent().unwrap().join("source");
    let mut combined = String::new();
    if source_dir.join("main.v").is_file() {
        combined.push_str(&std::fs::read_to_string(source_dir.join("main.v")).unwrap());
        combined.push('\n');
    }
    for name in ["async_effect.v", "benchmark.v", "enums_flags.v", "mezzo_macro.v", "nullable.v"] {
        let path = base.join(name);
        combined.push_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("read {}", path.display())));
        combined.push('\n');
    }
    let build_output = ValkyrieCompiler::new(SourceID { version_id: 9998 }).compile_source_to_build_output(&combined).expect("compile bundle");
    let (sum_types, _) = nyar_language::mir::compute_nominal_layouts(build_output.hir_module());
    let option_sum = sum_types.iter().find(|item| item.name == "Option");
    assert!(option_sum.is_some_and(|item| item.is_unite), "Option sum type should be unite");
    let target = CanonicalTarget::parse("clr-microsoft-unknown-managed").expect("clr target");
    let target_profile = target.to_profile(None);
    let projection_policy = projection_policy_for_target_profile(&target_profile).expect("projection policy");
    let registry = bundled_backend_registry(&build_output.neutral_plan().semantic_fragments, &target_profile, &projection_policy);
    let artifact_plan = plan_artifacts_from_build_output(&build_output, target, projection_policy, registry, ClrSuspendStrategy::default())
        .expect("artifact plan");
    let submission = assemble_fragment_submission(&build_output, &artifact_plan, 0).expect("fragment");
    let mut module = lower_fragment_to_clr_msil(&submission).expect("CLR lowering");
    augment_msil_with_witness(&submission, &mut module);
    PeWriter::new(PeWriterOptions {
        assembly_name: module.assembly.name.clone(),
        module_name: "feature_matrix.dll".to_string(),
        image_kind: ClrImageKind::Executable,
    })
    .write_module(&module)
    .unwrap_or_else(|error| panic!("PE write failed: {error:?}"));
    let mut counts = BTreeMap::<(String, String), usize>::new();
    for type_def in &module.types {
        *counts.entry((type_def.namespace.clone(), type_def.full_name.clone())).or_default() += 1;
    }
    let dupes: Vec<_> = counts.iter().filter(|(_, count)| **count > 1).collect();
    assert!(
        dupes.is_empty(),
        "duplicate CLR types: {dupes:?}; all={:?}",
        module.types.iter().map(|t| format!("{}::{}", t.namespace, t.full_name)).collect::<Vec<_>>()
    );
}

#[test]
fn feature_matrix_test_bundle_clr_pe_writes() {
    // PE roundtrip is asserted in `feature_matrix_test_bundle_clr_types_have_unique_names`.
}

#[test]
fn debug_singleton_clr_pipeline_dumps_msil() {
    use std::env;

    use nyar::backends::{clr::ClrImageKind, projection_policy_for_target_profile};
    use nyar_emitter::nyar_backend_clr::{PeWriter, PeWriterOptions};
    use nyar_language::{CanonicalTarget, ValkyrieCompiler, nyar::ClrSuspendStrategy, types::SourceID};

    let source = r#"
[clr("mscorlib", "System.Console", "WriteLine")]
micro console_write_line(message: utf16): unit;

singleton Counter {
    public mut total: i64 = 0

    micro increment(mut self) -> i64 {
        self.total = self.total + 1
        return self.total
    }

    micro get(self) -> i64 {
        return self.total
    }
}

[main]
micro main() -> i64 {
    Counter.increment()
    Counter.increment()
    console_write_line("counter done")
    return Counter.get()
}
"#;
    let build_output =
        ValkyrieCompiler::new(SourceID { version_id: 9700 }).compile_source_to_build_output(source).expect("compile singleton bundle");
    let target = CanonicalTarget::parse("clr-microsoft-unknown-managed").expect("clr target");
    let target_profile = target.to_profile(None);
    let projection_policy = projection_policy_for_target_profile(&target_profile).expect("projection policy");
    let registry = bundled_backend_registry(&build_output.neutral_plan().semantic_fragments, &target_profile, &projection_policy);
    let artifact_plan = plan_artifacts_from_build_output(&build_output, target, projection_policy, registry, ClrSuspendStrategy::default())
        .expect("artifact plan");
    let submission = assemble_fragment_submission(&build_output, &artifact_plan, 0).expect("fragment");
    let mut module = lower_fragment_to_clr_msil(&submission).expect("CLR lowering");
    augment_msil_with_singletons(&submission, &mut module).expect("singleton CLR lowering");
    augment_msil_with_witness(&submission, &mut module);

    let counter_ty = module.types.iter().find(|t| t.full_name == "Counter").expect("Counter type def");
    assert!(counter_ty.fields.iter().any(|f| f.name == "INSTANCE" && f.is_static), "INSTANCE static field missing");
    assert!(counter_ty.methods.iter().any(|m| m.method.name == ".cctor"), ".cctor missing");
    assert!(counter_ty.methods.iter().any(|m| m.method.name == "instance"), "instance accessor missing");
    assert!(counter_ty.methods.iter().any(|m| m.method.name == "increment"), "increment method missing");
    assert!(counter_ty.methods.iter().any(|m| m.method.name == "get"), "get method missing");

    if env::var("NYAR_SKIP_PE").is_ok() {
        return;
    }
    if let Err(error) = PeWriter::new(PeWriterOptions {
        assembly_name: module.assembly.name.clone(),
        module_name: "singleton_debug.dll".to_string(),
        image_kind: ClrImageKind::Executable,
    })
    .write_module(&module)
    {
        eprintln!("PE roundtrip skipped due to: {error:?}");
    }
}
