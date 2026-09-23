use nyar_language::{
    MirLowerer, MirOperation, MirStorageKind, ReceiverPassingKind, ValkyrieCompiler, compute_aggregate_layout_plan, layout_key_for_type,
    storage_kind_for_type,
    types::{Identifier, SourceID, hir::ValkyrieType},
};

#[test]
fn struct_new_marks_structure_as_value_storage() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9500 })
        .compile_source(
            r#"
structure Point {
    x: f64,
    y: f64,
}

micro main() {
    Point { x: 1.0, y: 2.0 }
}
"#,
        )
        .expect("compile");
    let mir = MirLowerer::lower_module(&hir);
    assert!(mir.structs.iter().any(|item| item.name == "Point" && item.is_value_type));
    assert!(mir.functions.iter().any(|function| {
        function.blocks.iter().any(|block| {
            block.instructions.iter().any(|ins| {
                matches!(
                    ins.kind,
                    MirOperation::StructNew { ref type_name, storage: MirStorageKind::Value, .. }
                        if type_name == "Point"
                )
            })
        })
    }));
}

#[test]
fn struct_new_marks_class_as_reference_storage() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9501 })
        .compile_source(
            r#"
class Node {
    next: Node?,
}

micro main() {
    Node { next: null }
}
"#,
        )
        .expect("compile");
    let mir = MirLowerer::lower_module(&hir);
    assert!(mir.structs.iter().any(|item| item.name == "Node" && !item.is_value_type));
    assert!(mir.functions.iter().any(|function| {
        function.blocks.iter().any(|block| {
            block.instructions.iter().any(|ins| {
                matches!(
                    ins.kind,
                    MirOperation::StructNew { ref type_name, storage: MirStorageKind::Reference, .. }
                        if type_name == "Node"
                )
            })
        })
    }));
}

#[test]
fn tuple_and_fixed_array_types_are_value_semantic() {
    let mut value_names = std::collections::BTreeSet::new();
    value_names.insert(Identifier::new("Point"));
    assert_eq!(storage_kind_for_type(&ValkyrieType::Tuple(vec![ValkyrieType::Boolean]), &value_names), MirStorageKind::Value);
    assert_eq!(
        storage_kind_for_type(
            &ValkyrieType::FixedArray { element: Box::new(ValkyrieType::Integer32 { signed: true }), length: 4 },
            &value_names
        ),
        MirStorageKind::Value
    );
    assert_eq!(
        storage_kind_for_type(&ValkyrieType::Array(Box::new(ValkyrieType::Integer32 { signed: true })), &value_names),
        MirStorageKind::Reference
    );
    assert_eq!(storage_kind_for_type(&ValkyrieType::Named(Identifier::new("Point")), &value_names), MirStorageKind::Value);
    assert_eq!(storage_kind_for_type(&ValkyrieType::Named(Identifier::new("Node")), &value_names), MirStorageKind::Reference);
}

#[test]
fn anonymous_structure_struct_new_uses_value_storage() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9502 })
        .compile_source(
            r#"
micro main() {
    structure { x: 1.0, y: 2.0 }
}
"#,
        )
        .expect("compile");
    let mir = MirLowerer::lower_module(&hir);
    assert!(mir.functions.iter().any(|function| {
        function.blocks.iter().any(|block| {
            block.instructions.iter().any(|ins| matches!(ins.kind, MirOperation::StructNew { storage: MirStorageKind::Value, .. }))
        })
    }));
}

#[test]
fn mir_lowering_merges_dynamic_fixed_array_layouts_into_module_plan() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9503 })
        .compile_source(
            r#"
micro main() -> i32 {
    let items: [i32; 2] = [1, 2];
    let copy = items;
    return copy[0];
}
"#,
        )
        .expect("compile");
    let static_plan = compute_aggregate_layout_plan(&hir);
    let mir = MirLowerer::lower_module_semantic(&hir);
    assert!(
        mir.aggregate_layouts.layouts.len() > static_plan.layouts.len(),
        "MIR lowering should register fixed-array layouts absent from the HIR-only plan"
    );
    let fixed_array_key =
        layout_key_for_type(&ValkyrieType::FixedArray { element: Box::new(ValkyrieType::Integer32 { signed: true }), length: 2 })
            .expect("fixed-array layout key");
    assert!(
        mir.aggregate_layouts.type_name_to_layout.contains_key(&fixed_array_key),
        "expected fixed-array layout key `{fixed_array_key}` in mir.aggregate_layouts"
    );
}

#[test]
fn aggregate_copy_layout_ids_resolve_in_mir_module_plan() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9504 })
        .compile_source(
            r#"
structure Point {
    x: f64,
    y: f64,
}

micro main() {
    let a = Point { x: 1.0, y: 2.0 };
    let b = a;
}
"#,
        )
        .expect("compile");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let copy_layout_ids = mir
        .functions
        .iter()
        .flat_map(|function| function.blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .filter_map(|instruction| match instruction.kind {
            MirOperation::AggregateCopy { layout_id, .. } => Some(layout_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!copy_layout_ids.is_empty(), "expected value assignment to lower to AggregateCopy");
    for layout_id in copy_layout_ids {
        assert!(
            mir.aggregate_layouts.layouts.iter().any(|layout| layout.id == layout_id),
            "AggregateCopy layout_id `{layout_id}` must exist in mir.aggregate_layouts"
        );
    }
}

/// Build a CLR bundled `BackendRegistry` mirroring `nyar_emitter::bundled_backend_registry`.
///
/// `nyar-language` tests cannot depend on `emitter`, so the CLR capability descriptor is
/// reconstructed inline from the same constants used by the driver's bundled CLR backend.
fn clr_bundled_registry(
    fragments: &[nyar_language::nyar::SemanticFragment],
    target_profile: &nyar_language::TargetProfile,
    projection_policy: &nyar_language::nyar::ProjectionPolicy,
) -> nyar_language::nyar::BackendRegistry {
    use nyar_language::nyar::{
        BackendCapability, BackendInputKind, BackendInterpreterRegistration, BinaryTarget, HostProjectionBoundary, ReferenceManagement,
        TargetLane,
    };
    let mut registry = nyar_language::nyar::BackendRegistry::default();
    let binary_target: BinaryTarget = target_profile.canonical_target.into();
    for fragment in fragments {
        registry.register(BackendInterpreterRegistration {
            backend_name: "clr-binary".to_string(),
            priority: 100,
            capability: BackendCapability {
                interpreter: Identifier::new("clr.msil"),
                fragment: fragment.id.clone(),
                lane: TargetLane::Clr,
                input_kind: Some(BackendInputKind::MsilText),
                supported_projection_families: vec![projection_policy.family],
                supported_host_boundaries: vec![HostProjectionBoundary::Clr],
                supported_targets: vec![binary_target.clone()],
                required_capabilities: fragment.required_capabilities.clone(),
                reference_management: Some(ReferenceManagement::HostGc),
            },
        });
    }
    registry
}

#[test]
fn backend_fragment_carries_mir_final_aggregate_layouts() {
    use nyar_language::{
        assemble_fragment,
        nyar::{ClrSuspendStrategy, backends::projection_policy_for_target_profile},
        plan_artifacts_from_build_output,
    };

    let source = r#"
structure Point {
    x: f64,
    y: f64,
}

micro main() {
    let a = Point { x: 1.0, y: 2.0 };
    let b = a;
}
"#;
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 9505 });
    let build_output = compiler.compile_source_to_build_output(source).expect("build output");
    let mir = MirLowerer::lower_module_semantic(build_output.hir_module());
    let target = nyar_language::CanonicalTarget::parse("clr-microsoft-unknown-managed").expect("clr target");
    let target_profile = target.to_profile(None);
    let projection_policy = projection_policy_for_target_profile(&target_profile).expect("projection policy");
    let backend_registry = clr_bundled_registry(&build_output.neutral_plan().semantic_fragments, &target_profile, &projection_policy);
    let artifact_plan =
        plan_artifacts_from_build_output(&build_output, target, projection_policy, backend_registry, ClrSuspendStrategy::default())
            .expect("artifact plan");
    let fragment = assemble_fragment(&build_output, &artifact_plan, 0).expect("backend fragment");
    assert_eq!(
        fragment.aggregate_layouts, mir.aggregate_layouts,
        "AssembledFragment must reuse MIR-final aggregate_layouts, not a HIR-only recompute"
    );
}

#[test]
fn value_field_access_emits_layout_id() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9506 })
        .compile_source(
            r#"
structure Point {
    x: f64,
    y: f64,
}

micro main() -> f64 {
    let p = Point { x: 1.0, y: 2.0 };
    return p.x;
}
"#,
        )
        .expect("compile");
    let mir = MirLowerer::lower_module_semantic(&hir);
    assert!(mir.functions.iter().any(|function| {
        function.blocks.iter().any(|block| {
            block.instructions.iter().any(|ins| {
                matches!(
                    ins.kind,
                    MirOperation::FieldGet { ref field, storage: MirStorageKind::Value, layout_id: Some(_), .. }
                        if field == "x"
                )
            })
        })
    }));
}

#[test]
fn tuple_call_emits_tuple_new_with_layout_id() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9507 })
        .compile_source(
            r#"
micro main() {
    let pair = tuple(1, 2);
}

"#,
        )
        .expect("compile");
    let mir = MirLowerer::lower_module_semantic(&hir);
    assert!(mir.functions.iter().any(|function| {
        function.blocks.iter().any(|block| {
            block
                .instructions
                .iter()
                .any(|ins| matches!(ins.kind, MirOperation::TupleNew { storage: MirStorageKind::Value, layout_id: Some(_), .. }))
        })
    }));
    let tuple_layout_ids = mir
        .functions
        .iter()
        .flat_map(|function| function.blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .filter_map(|instruction| match instruction.kind {
            MirOperation::TupleNew { layout_id: Some(layout_id), .. } => Some(layout_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!tuple_layout_ids.is_empty(), "expected TupleNew with layout_id");
    for layout_id in tuple_layout_ids {
        assert!(
            mir.aggregate_layouts.layouts.iter().any(|layout| layout.id == layout_id),
            "TupleNew layout_id `{layout_id}` must exist in mir.aggregate_layouts"
        );
    }
}

#[test]
fn repeated_nominal_tuple_keeps_structural_layout_identity() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9509 })
        .compile_source(
            r#"
structure Point {
    x: i32,
}

micro main() {
    let p = Point { x: 1 };
    let pair = tuple(p, p);
}
"#,
        )
        .expect("compile");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let tuple_key = layout_key_for_type(&ValkyrieType::Tuple(vec![
        ValkyrieType::Named(Identifier::new("Point")),
        ValkyrieType::Named(Identifier::new("Point")),
    ]))
    .expect("tuple layout key");
    let tuple_layout = mir.aggregate_layouts.layouts.iter().find(|layout| layout.name == tuple_key).expect("repeated tuple layout");
    let point_layout = mir.aggregate_layouts.layouts.iter().find(|layout| layout.name == "Point").expect("nominal layout");
    assert_ne!(tuple_layout.id, point_layout.id, "(Point, Point) must not reuse Point layout");
    assert_eq!(tuple_layout.fields.len(), 2, "repeated tuple must expose both tuple fields");
}

#[test]
fn value_type_method_call_emits_by_address_receiver() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9508 })
        .compile_source(
            r#"
structure Point {
    x: f64,
    y: f64,
}

imply Point {
    micro length(self) -> f64 {
        return self.x;
    }
}

micro main() -> f64 {
    let p = Point { x: 1.0, y: 2.0 };
    return p.length();
}
"#,
        )
        .expect("compile");
    let mir = MirLowerer::lower_module_semantic(&hir);
    assert!(mir.functions.iter().any(|function| {
        function.blocks.iter().any(|block| {
            block
                .instructions
                .iter()
                .any(|ins| matches!(ins.kind, MirOperation::Call {})),
        })
    }));
}
