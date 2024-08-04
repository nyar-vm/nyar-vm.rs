use nyar_language::{
    ValkyrieCompiler,
    mir::validation::validate_semantic_module,
    types::{
        Identifier,
        hir::{FunctionType, ValkyrieType},
    },
    valkyrie::mir::ssa::{MirLowerer, MirModule, MirOperand, MirOperation},
};

fn source_module() -> MirModule {
    let hir = ValkyrieCompiler::default()
        .compile_source(
            "class Parcel<First, Second> { head: First, tail: Second, items: [Second] } \
         micro head(value: Parcel<bool, utf8>) -> bool { return value.head } \
         micro tail(value: Parcel<bool, utf8>) -> utf8 { return value.tail } \
         micro items(value: Parcel<bool, utf8>) -> [utf8] { return value.items }",
        )
        .expect("字段源码必须通过 HIR");
    let module = MirLowerer::lower_module_semantic(&hir);
    assert_eq!(module.structs[0].generics, hir.structs[0].generics);
    validate_semantic_module(&module).expect("字段结果必须符合完整声明代入");
    module
}

fn parcel_nominal(module: &MirModule, arguments: Vec<ValkyrieType>) -> ValkyrieType {
    let declaration = module.structs[0].declaration.expect("Parcel 须冻结 constructor_declaration");
    ValkyrieType::Nominal { declaration, arguments }
}

fn field_id(module: &MirModule, name: &str) -> nyar_types::FieldId {
    module
        .structs
        .iter()
        .flat_map(|structure| &structure.fields)
        .find(|field| field.name == name)
        .map(|field| field.id)
        .expect("field identity")
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn source_generic_field_reads_preserve_all_binders_without_class_dereference() {
    let module = source_module();
    let expected =
        [("head", ValkyrieType::Boolean), ("tail", ValkyrieType::Utf8), ("items", ValkyrieType::Array(Box::new(ValkyrieType::Utf8)))];
    let field_ids = expected.iter().map(|(name, _)| (*name, field_id(&module, name))).collect::<std::collections::BTreeMap<_, _>>();
    let owner = parcel_nominal(&module, vec![ValkyrieType::Boolean, ValkyrieType::Utf8]);
    for (name, ty) in expected {
        let mut matches = 0;
        for function in &module.functions {
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                match &instruction.kind {
                    MirOperation::FieldGet { object, field } if *field == field_ids[name] => {
                        matches += 1;
                        assert_eq!(instruction.results.len(), 1);
                        assert_eq!(function.value_types[&instruction.results[0]], ty);
                        let MirOperand::Value(object) = object
                        else {
                            panic!("字段对象必须是 SSA 值");
                        };
                        assert_eq!(function.value_types[object], owner);
                    }
                    MirOperation::Call { .. } => panic!("直接字段读取不得补造 class 解引用调用"),
                    _ => {}
                }
            }
        }
        assert_eq!(matches, 1);
    }
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn semantic_field_contract_rejects_wrong_result_and_missing_owner_despite_layout() {
    let module = source_module();
    let mut missing = module.clone();
    missing.structs.clear();
    // 开放泛型不再登记平台布局；注入伪布局，证明物理侧表仍不能代替语义声明。
    if missing.aggregate_layouts.layouts.is_empty() {
        missing.aggregate_layouts.layouts.push(nyar_language::mir::AggregateLayout {
            id: 1,
            name: "Parcel".into(),
            namespace: String::new(),
            key: None,
            storage: nyar_language::mir::MirStorageKind::Reference,
            size: 8,
            align: 8,
            fields: Vec::new(),
        });
    }
    assert!(!missing.aggregate_layouts.layouts.is_empty());
    assert_eq!(validate_semantic_module(&missing).expect_err("物理布局不能代替语义声明").code, "SMIR006");
    let mut wrong = module;
    let function = wrong.functions.iter_mut().find(|function| function.return_type == ValkyrieType::Utf8).unwrap();
    let result = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| matches!(&instruction.kind, MirOperation::FieldGet { .. }).then(|| instruction.results[0]))
        .unwrap();
    function.value_types.insert(result, ValkyrieType::Boolean);
    function.return_type = ValkyrieType::Boolean;
    assert_eq!(validate_semantic_module(&wrong).expect_err("同形或返回合同变化不能掩盖字段错位").code, "SMIR006");
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn semantic_field_contract_rejects_missing_arguments_and_tuple_wrapping() {
    let module = source_module();
    for owner in [
        parcel_nominal(&module, vec![ValkyrieType::Boolean]),
        ValkyrieType::Tuple(vec![parcel_nominal(&module, vec![ValkyrieType::Boolean, ValkyrieType::Utf8])]),
    ] {
        let mut invalid = module.clone();
        let function = &mut invalid.functions[0];
        function.param_types[0] = owner.clone();
        function.value_types.insert(function.blocks[0].parameters[0], owner);
        assert_eq!(validate_semantic_module(&invalid).expect_err("缺实参或 tuple 不得冒充名义 owner").code, "SMIR006");
    }
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn declared_fields_instantiate_function_fixed_array_and_self_without_layout() {
    let mut declaration = source_module().structs.remove(0);
    let first = declaration.fields[0].ty.clone();
    let second = declaration.fields[1].ty.clone();
    let item = declaration.declaration.expect("Parcel declaration");
    let owner = ValkyrieType::Nominal { declaration: item, arguments: vec![ValkyrieType::Boolean, ValkyrieType::Utf8] };
    declaration.fields[0].ty = ValkyrieType::Function(Box::new(FunctionType { params: vec![first], return_type: second.clone() }));
    assert_eq!(
        declaration.instantiate_field(&owner, "head"),
        Some(ValkyrieType::Function(Box::new(FunctionType { params: vec![ValkyrieType::Boolean], return_type: ValkyrieType::Utf8 })))
    );
    declaration.fields[1].ty = ValkyrieType::FixedArray { element: Box::new(second), length: 3 };
    assert_eq!(
        declaration.instantiate_field(&owner, "tail"),
        Some(ValkyrieType::FixedArray { element: Box::new(ValkyrieType::Utf8), length: 3 })
    );
    declaration.fields[2].ty = ValkyrieType::Nullable(Box::new(ValkyrieType::SelfType));
    assert_eq!(declaration.instantiate_field(&owner, "items"), Some(ValkyrieType::Nullable(Box::new(owner))));
}

#[test]
fn source_field_declarations_bind_by_item_id_and_reject_named_owners() {
    // 同一编译单元内分配 ItemId；分开 compile_source 会各自从 0 起号，不能拿来证明跨声明唯一性。
    let hir = ValkyrieCompiler::default()
        .compile_source(
            "namespace first; structure Item { value: bool }\n\
             namespace second; structure Item { value: bool }",
        )
        .expect("同编译单元内不同 namespace 的同名声明");
    let declarations = MirLowerer::lower_module_semantic(&hir).structs;
    assert_eq!(declarations.len(), 2);
    let first_id = declarations[0].declaration.expect("first Item declaration");
    let second_id = declarations[1].declaration.expect("second Item declaration");
    assert_ne!(first_id, second_id, "同编译单元内不同声明必须有不同 ItemId");
    for declaration in &declarations {
        let item = declaration.declaration.expect("Item 须冻结 declaration");
        let owner = ValkyrieType::Nominal { declaration: item, arguments: Vec::new() };
        assert_eq!(declaration.instantiate_field(&owner, "value"), Some(ValkyrieType::Boolean));
        // Named 短名/限定名都不是 owner 身份。
        assert_eq!(declaration.instantiate_field(&ValkyrieType::Named(Identifier::new("Item")), "value"), None);
        assert_eq!(declaration.instantiate_field(&ValkyrieType::Named(Identifier::new(&declaration.qualified_name())), "value"), None);
    }
    let owner = ValkyrieType::Nominal { declaration: second_id, arguments: Vec::new() };
    assert_eq!(declarations[0].instantiate_field(&owner, "value"), None);
}

#[test]
fn source_field_declaration_reaches_canonical_without_a_construction_seed() {
    let output = ValkyrieCompiler::default()
        .compile_source_to_program("structure Flag { value: bool } micro read(flag: Flag) -> bool { return flag.value }")
        .expect("当前源码字段声明必须贯穿 Compiler 成功载荷");
    let program = output.canonical();
    program.validate().expect("Canonical 完整字段身份与结果合同");
    let mut reads = 0;
    for function in program.mir.functions.values() {
        for instruction in function.blocks.values().flat_map(|block| &block.instructions) {
            if let nyar_types::CanonicalOperation::FieldGet { field, .. } = &instruction.operation {
                reads += 1;
                let record = &program.linked.fields[field];
                assert_eq!(record.ty, function.value_types[&instruction.results[0]]);
                assert!(output.representation().adt_reps.contains_key(&record.owner));
            }
        }
    }
    assert_eq!(reads, 1);
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn sum_param_binds_declaration_identity_through_canonical() {
    // 避免 match 穷尽性噪声；只验收 sum 参数已冻结 Nominal{ItemId}。
    let source = "unite Envelope<T> { Present { value: T }, Absent } \
             micro identity(box: Envelope<utf8>) -> Envelope<utf8> { return box }";
    let mir = ValkyrieCompiler::default().compile_source_to_mir(source).expect("Semantic MIR");
    let sum = mir.sum_types.iter().find(|item| item.name == "Envelope").expect("Envelope");
    let item = sum.declaration.expect("Envelope declaration ItemId");
    let expected = ValkyrieType::Nominal { declaration: item, arguments: vec![ValkyrieType::Utf8] };
    let function = mir.functions.iter().find(|f| f.display_symbol.contains("identity")).expect("identity");
    assert_eq!(function.param_types[0], expected, "sum 参数须是 Nominal{{ItemId}}，不得残留 Named/Apply(Named)");
    assert_eq!(function.return_type, expected, "sum 返回类型须与参数共享同一 Nominal 声明");
    let monomorphic = ValkyrieType::Nominal { declaration: item, arguments: Vec::new() };
    assert!(mir.type_identities.contains_key(&monomorphic), "sum 声明键须进入 type identity 表");
    assert!(mir.type_identities.contains_key(&expected), "具体 sum 实例须进入 type identity 表");
    assert!(!mir.type_identities.contains_key(&ValkyrieType::Named(Identifier::new("Envelope"))));
    let output = ValkyrieCompiler::default().compile_source_to_program(source).expect("sum 参数必须贯穿 Compiler");
    output.canonical().validate().expect("sum Nominal 身份须通过 Canonical");
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn option_sum_construct_binds_declaration_identity_without_short_name() {
    // 不依赖 unwrap 方法 imply；验收 Option 构造与参数已冻结 Nominal{ItemId}，且 SumNew payload 合同闭合。
    let source = "unite Option<T> { Some { value: T }, None } \
             micro wrap(value: utf8) -> Option<utf8> { return Some { value: value } } \
             micro identity(box: Option<utf8>) -> Option<utf8> { return box }";
    let mir = ValkyrieCompiler::default().compile_source_to_mir(source).unwrap_or_else(|err| panic!("Option sum 源码失败: {err:?}"));
    let option = mir.sum_types.iter().find(|item| item.name.contains("Option")).expect("Option");
    let item = option.declaration.expect("Option declaration");
    let expected = ValkyrieType::Nominal { declaration: item, arguments: vec![ValkyrieType::Utf8] };
    let wrap = mir.functions.iter().find(|f| f.display_symbol.contains("wrap")).expect("wrap");
    assert_eq!(wrap.return_type, expected, "Option 返回须是 Nominal{{ItemId}}");
    let identity = mir.functions.iter().find(|f| f.display_symbol.contains("identity")).expect("identity");
    assert_eq!(identity.param_types[0], expected);
    assert!(!mir.type_identities.contains_key(&ValkyrieType::Named(Identifier::new("Option"))));
    assert!(mir.type_identities.contains_key(&expected));
    validate_semantic_module(&mir).expect("Option SumNew 合同");
    let output = ValkyrieCompiler::default().compile_source_to_program(source).expect("Option 须贯穿 Compiler");
    output.canonical().validate().expect("Option Nominal 身份与 SumNew payload 须通过 Canonical");
}

#[test]
fn namespaced_struct_param_binds_declaration_identity_through_canonical() {
    let source = "namespace core.text; structure TextSpan { start: i32, end: i32 } \
             micro as_start(span: TextSpan) -> i32 { return span.start }";
    let mir = ValkyrieCompiler::default().compile_source_to_mir(source).expect("Semantic MIR");
    let structure = mir.structs.iter().find(|item| item.name == "TextSpan").expect("TextSpan");
    let item = structure.declaration.expect("TextSpan declaration ItemId");
    let declaration_ty = ValkyrieType::Nominal { declaration: item, arguments: Vec::new() };
    let declaration_id = mir.type_identities.get(&declaration_ty).copied().expect("Nominal 声明键须进入 type identity 表");
    let param_ty = mir.functions.iter().find(|f| f.display_symbol.contains("as_start")).expect("as_start").param_types[0].clone();
    assert_eq!(param_ty, declaration_ty, "参数须是已绑定 ItemId 的 Nominal，不得残留 Named 短名");
    let param_id = mir.type_identities.get(&param_ty).copied().expect("参数 Nominal 须进入 type identity 表");
    assert_eq!(param_id, declaration_id, "参数与聚合声明须共享同一 TypeId");
    assert!(!mir.type_identities.contains_key(&ValkyrieType::Named(Identifier::new("TextSpan"))));
    assert!(!mir.type_identities.contains_key(&ValkyrieType::Named(Identifier::new("core.text.TextSpan"))));
    let output = ValkyrieCompiler::default().compile_source_to_program(source).expect("命名空间结构字段读取必须贯穿 Compiler");
    output.canonical().validate().expect("不得再因 FieldOwnerMismatch 失败");
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn semantic_fields_reject_duplicate_declarations_binders_and_fields() {
    let module = source_module();
    let mut owner = module.clone();
    owner.structs.push(module.structs[0].clone());
    let mut binder = module.clone();
    binder.structs[0].generics.push(module.structs[0].generics[0].clone());
    let mut field = module.clone();
    field.structs[0].fields.push(module.structs[0].fields[0].clone());
    for invalid in [owner, binder, field] {
        assert_eq!(validate_semantic_module(&invalid).expect_err("声明合同不得 first-wins").code, "SMIR006");
    }
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn semantic_field_write_rejects_owner_value_masquerading_as_field_type() {
    // Oak 尚未稳定产出 StoreField（`obj.field = x`）；此处直接构造 FieldSet，
    // 验收 Semantic MIR 合同：写入值类型必须匹配字段声明代入，不得用 owner 对象冒充。
    let mut module = ValkyrieCompiler::default()
        .compile_source_to_mir(
            "class Parcel<First, Second> { head: First, tail: Second } \
         micro read(value: Parcel<bool, utf8>) -> utf8 { return value.tail }",
        )
        .expect("泛型字段读取源码必须满足 Semantic MIR 合同");
    let field = field_id(&module, "tail");
    let function = module.functions.iter_mut().find(|function| function.display_symbol.contains("read")).expect("read");
    let object = function.blocks[0].parameters[0];
    function.blocks[0].instructions.push(nyar_language::valkyrie::mir::ssa::MirInstruction::from_operation(MirOperation::FieldSet {
        object: MirOperand::Value(object),
        field,
        value: MirOperand::Value(object),
    }));
    assert_eq!(validate_semantic_module(&module).expect_err("写入对象不能冒充声明的 utf8 字段值").code, "SMIR006");
}
