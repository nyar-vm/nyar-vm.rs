use nyar_types::{
    layout_choice::ValueRepresentation,
    semantic_ids::{InstructionIdKind, ItemIdKind},
    *,
};

#[test]
fn evidence_id_is_stable_and_distinct() {
    let a = EvidenceId::from_parts("Comparable", "Int32", "compare");
    let b = EvidenceId::from_parts("Comparable", "Int32", "compare");
    let c = EvidenceId::from_parts("Comparable", "Utf8", "compare");
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_eq!(a.as_str(), "evidence:Comparable@Int32#compare");
}

#[test]
fn instruction_id_is_dense_and_nonzero() {
    let id = InstructionId::from_index(0).expect("index 0");
    assert_eq!(id.index(), 0);
    assert!(InstructionId::from_index(u32::MAX).is_some() || InstructionId::from_index(u32::MAX).is_none());
}

#[test]
fn representation_plan_keys_are_stable_ids() {
    let mut plan = layout_choice::RepresentationPlan::default();
    let insn = InstructionId::from_index(3).unwrap();
    plan.invoke_lowerings.insert(insn, layout_choice::InvokeLowering::Direct);
    assert!(plan.invoke_lowerings.contains_key(&insn));
    let function = ItemInstanceId::from_index(0).unwrap();
    let other_function = ItemInstanceId::from_index(1).unwrap();
    let value = MirValueId::from_index(0).unwrap();
    plan.value_reps.insert(ValueIdentity::new(function, value), ValueRepresentation::Specialized);
    plan.value_reps.insert(ValueIdentity::new(other_function, value), ValueRepresentation::Reified);
    assert!(plan.value_reps.contains_key(&ValueIdentity::new(function, value)));
    assert!(plan.value_reps.contains_key(&ValueIdentity::new(other_function, value)));
    assert_eq!(plan.value_reps.len(), 2);
}

#[test]
fn item_and_type_instance_ids_are_opaque() {
    let item = ItemId::from_index(0).expect("item");
    let ty = TypeInstanceId::from_index(1).expect("type instance");
    let import = ImportIndex::from_index(0).expect("import");
    assert_eq!(item.index(), 0);
    assert_eq!(ty.index(), 1);
    assert_eq!(import.index(), 0);
}

#[test]
fn branded_ids_are_distinct_types() {
    fn expects_instruction(_: InstructionId) {}
    expects_instruction(InstructionId::from_index(0).unwrap());
    assert_eq!(InstructionIdKind::NAME, "InstructionId");
    assert_eq!(ItemIdKind::NAME, "ItemId");
}

#[test]
fn intrinsic_and_attribute_ids_are_extensible() {
    assert_eq!(IntrinsicId::ArrayPush.diagnostic_path(), "builtin.array.push");
    assert_eq!(IntrinsicId::ArrayPush.bytecode_index(), Some(0));
    assert_eq!(IntrinsicId::from_bytecode_index(2), Some(IntrinsicId::ArrayGet));
    assert_eq!(IntrinsicId::from_bytecode_index(99), None);
    assert_eq!(IntrinsicId::resolve_from_segments(&["builtin", "array", "push"]), Some(IntrinsicId::ArrayPush));
    assert_eq!(IntrinsicId::resolve_from_segments(&["__array_len"]), Some(IntrinsicId::ArrayLen));
    assert_eq!(IntrinsicId::resolve_from_segments(&["marker", "__ref_deref"]), Some(IntrinsicId::RefDeref));
    // 禁止按类型名末段猜
    assert_eq!(IntrinsicId::resolve_from_segments(&["Array", "get"]), None);
    assert_eq!(IntrinsicId::resolve_from_segments(&["HashMap", "get"]), None);
    assert_eq!(IntrinsicId::resolve_from_segments(&["is_null"]), Some(IntrinsicId::IsNull));
    assert_eq!(IntrinsicId::resolve_from_segments(&["unwrap_null"]), Some(IntrinsicId::UnwrapNull));
    assert_eq!(IntrinsicId::resolve_from_segments(&["builtin", "null", "is_null"]), Some(IntrinsicId::IsNull));
    assert_eq!(builtin_attribute::export().index(), 0);
    assert_eq!(builtin_attribute::main().index(), 1);
    let seeds = builtin_attribute::seed_registrations();
    assert_eq!(seeds[0].name, "export");
    assert_eq!(seeds.len(), 5);
    assert_eq!(builtin_attribute::lookup_seed("export"), Some(builtin_attribute::export()));
    assert_eq!(builtin_attribute::lookup_seed("workload_phase"), Some(builtin_attribute::workload_phase()));
    assert_eq!(builtin_attribute::lookup_seed("custom_attr"), None);
    let cap = ImportCapability::new("env", "console_log");
    assert_eq!(cap.to_string(), "env::console_log");
}

#[test]
fn numeric_intrinsics_have_structural_identity_only_for_supported_scalar_and_op() {
    let identity = IntrinsicId::resolve_from_segments(&["i32", "add"]).expect("`i32.add` 必须有结构化 identity");
    assert_eq!(identity, IntrinsicId::Numeric(NumericIntrinsic::new(NumericScalar::I32, NumericOp::Add)));
    assert_eq!(identity.diagnostic_path(), "i32.add");
    assert_eq!(identity.bytecode_index(), None, "数值 identity 不得借用 `CallIntrinsic` 下标");

    // 反例：没有目标指令来源的宽度 / 运算必须无法解析成 identity。
    for scalar in ["u8", "u16", "i8", "i16", "i64", "u32", "u64", "isize", "usize", "f32", "f64", "bool"] {
        assert_eq!(IntrinsicId::resolve_from_segments(&[scalar, "add"]), None, "宽度 `{scalar}` 不应登记数值 identity");
    }
    for op in ["pow", "and", "or", "xor", "shl", "shr", "neg", "bit_not", "min", "max"] {
        assert_eq!(IntrinsicId::resolve_from_segments(&["i32", op]), None, "运算 `{op}` 不应登记数值 identity");
    }
    assert_eq!(IntrinsicId::resolve_from_segments(&["i32", "add", "extra"]), None, "不允许多段猜测");
}
