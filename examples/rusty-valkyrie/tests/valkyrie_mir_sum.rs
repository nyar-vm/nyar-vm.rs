use rusty_valkyrie::{
    ValkyrieCompiler,
    mir::{MirLowerer, MirOperation, validation::validate_semantic_module},
    types::hir::types::ValkyrieType,
    *,
};

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn sum_declaration_preserves_generic_fields_and_instantiates_without_variant_spelling() {
    let hir = ValkyrieCompiler::default()
        .compile_source(
            r#"
unite Parcel<First, Second> {
    Arbitrary { head: Second, tail: [First] },
    Empty,
}
"#,
        )
        .expect("当前源码声明");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let sum = &mir.sum_types[0];
    let variant = &sum.variants[0];
    assert_eq!(sum.generics, hir.enums[0].generics);
    assert_eq!(variant.fields[0].name, "head");
    assert_eq!(variant.fields[1].name, "tail");
    assert_eq!(
        sum.instantiate_payload(variant, &[ValkyrieType::Boolean, ValkyrieType::Utf8]),
        Some(Some(ValkyrieType::Tuple(vec![ValkyrieType::Utf8, ValkyrieType::Array(Box::new(ValkyrieType::Boolean))])))
    );
    assert!(sum.instantiate_payload(variant, &[ValkyrieType::Boolean]).is_none());
    validate_semantic_module(&mir).expect("声明合同");
}

#[test]
fn sum_declaration_rejects_duplicate_owner_binder_and_field() {
    let hir = ValkyrieCompiler::default().compile_source("unite Parcel<T> { Arbitrary { value: T } }").expect("当前源码声明");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let mut duplicate_owner = mir.clone();
    duplicate_owner.sum_types.push(mir.sum_types[0].clone());
    assert_eq!(validate_semantic_module(&duplicate_owner).unwrap_err().code, "SMIR006");
    let mut duplicate_binder = mir.clone();
    duplicate_binder.sum_types[0].generics.push(mir.sum_types[0].generics[0].clone());
    assert_eq!(validate_semantic_module(&duplicate_binder).unwrap_err().code, "SMIR006");
    let mut duplicate_field = mir.clone();
    duplicate_field.sum_types[0].variants[0].fields.push(mir.sum_types[0].variants[0].fields[0].clone());
    assert_eq!(validate_semantic_module(&duplicate_field).unwrap_err().code, "SMIR006");
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn sum_declaration_preserves_variant_result_refinement() {
    let mut hir = ValkyrieCompiler::default().compile_source("unite Parcel<T> { Arbitrary { value: T } }").expect("当前源码声明");
    let item = hir.enums[0].declaration.expect("Parcel declaration");
    let refined = ValkyrieType::Nominal { declaration: item, arguments: vec![ValkyrieType::Boolean] };
    hir.enums[0].variants[0].result_type = Some(refined.clone());
    let mir = MirLowerer::lower_module_semantic(&hir);
    let sum = &mir.sum_types[0];
    assert_eq!(sum.variants[0].result_type, Some(refined.clone()));
    assert_eq!(sum.instantiate_result(&sum.variants[0], &[ValkyrieType::Utf8]), Some(refined));
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn sum_operations_from_source_reject_missing_substitution_and_wrong_payload() {
    let mir = ValkyrieCompiler::default()
        .compile_source_to_mir(
            r#"
unite Envelope<T> { Present { value: T }, Absent }
micro wrap(value: utf8) -> Envelope<utf8> {
    return Present { value: value }
}
"#,
        )
        .expect("当前源码构造合同");
    validate_semantic_module(&mir).expect("正确实例化");
    let mut missing = mir.clone();
    let operation = missing
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .flat_map(|block| &mut block.instructions)
        .find_map(|instruction| match &mut instruction.kind {
            MirOperation::SumNew { type_args, .. } => Some(type_args),
            _ => None,
        })
        .expect("SumNew");
    operation.clear();
    assert_eq!(validate_semantic_module(&missing).unwrap_err().code, "SMIR006");
    let mut wrong = mir;
    let operation = wrong
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .flat_map(|block| &mut block.instructions)
        .find_map(|instruction| match &mut instruction.kind {
            MirOperation::SumNew { payload_type, .. } => Some(payload_type),
            _ => None,
        })
        .expect("SumNew");
    *operation = Some(ValkyrieType::Boolean);
    assert_eq!(validate_semantic_module(&wrong).unwrap_err().code, "SMIR006");
}
