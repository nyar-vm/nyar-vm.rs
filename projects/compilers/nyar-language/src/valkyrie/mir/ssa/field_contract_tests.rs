use super::{MirLowerer, MirModule, MirOperand, MirOperation, MirStruct};
use crate::{ValkyrieCompiler, types::{Identifier, hir::{FunctionType, ValkyrieType}}};
use crate::mir::validation::validate_semantic_module;

fn source_module() -> MirModule {
    let hir = ValkyrieCompiler::default().compile_source(
        "class Parcel<First, Second> { head: First, tail: Second, items: [Second] } \
         micro head(value: Parcel<bool, utf8>) -> bool { return value.head } \
         micro tail(value: Parcel<bool, utf8>) -> utf8 { return value.tail } \
         micro items(value: Parcel<bool, utf8>) -> [utf8] { return value.items }",
    ).expect("字段源码必须通过 HIR");
    let module = MirLowerer::lower_module_semantic(&hir);
    assert_eq!(module.structs[0].generics, hir.structs[0].generics);
    validate_semantic_module(&module).expect("字段结果必须符合完整声明代入");
    module
}

fn applied(name: &str, arguments: Vec<ValkyrieType>) -> ValkyrieType {
    ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new(name))), arguments)
}

#[test]
fn source_generic_field_reads_preserve_all_binders_without_class_dereference() {
    let module = source_module();
    let expected = [
        ("head", ValkyrieType::Boolean),
        ("tail", ValkyrieType::Utf8),
        ("items", ValkyrieType::Array(Box::new(ValkyrieType::Utf8))),
    ];
    for (name, ty) in expected {
        let mut matches = 0;
        for function in &module.functions {
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                match &instruction.kind {
                    MirOperation::FieldGet { object, field } if field.as_str() == name => {
                        matches += 1;
                        assert_eq!(instruction.results.len(), 1);
                        assert_eq!(function.value_types[&instruction.results[0]], ty);
                        let MirOperand::Value(object) = object else { panic!("字段对象必须是 SSA 值"); };
                        assert_eq!(function.value_types[object], applied("Parcel", vec![ValkyrieType::Boolean, ValkyrieType::Utf8]));
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
fn semantic_field_contract_rejects_wrong_result_and_missing_owner_despite_layout() {
    let module = source_module();
    let mut missing = module.clone();
    missing.structs.clear();
    assert!(!missing.aggregate_layouts.layouts.is_empty());
    assert_eq!(validate_semantic_module(&missing).expect_err("物理布局不能代替语义声明").code, "SMIR006");
    let mut wrong = module;
    let function = wrong.functions.iter_mut().find(|function| function.return_type == ValkyrieType::Utf8).unwrap();
    let result = function.blocks.iter().flat_map(|block| &block.instructions).find_map(|instruction| {
        matches!(&instruction.kind, MirOperation::FieldGet { .. }).then(|| instruction.results[0])
    }).unwrap();
    function.value_types.insert(result, ValkyrieType::Boolean);
    function.return_type = ValkyrieType::Boolean;
    assert_eq!(validate_semantic_module(&wrong).expect_err("同形或返回合同变化不能掩盖字段错位").code, "SMIR006");
}

#[test]
fn semantic_field_contract_rejects_missing_arguments_and_tuple_wrapping() {
    let module = source_module();
    for owner in [
        applied("Parcel", vec![ValkyrieType::Boolean]),
        ValkyrieType::Tuple(vec![applied("Parcel", vec![ValkyrieType::Boolean, ValkyrieType::Utf8])]),
    ] {
        let mut invalid = module.clone();
        let function = &mut invalid.functions[0];
        function.param_types[0] = owner.clone();
        function.value_types.insert(function.blocks[0].parameters[0], owner);
        assert_eq!(validate_semantic_module(&invalid).expect_err("缺实参或 tuple 不得冒充名义 owner").code, "SMIR006");
    }
}

#[test]
fn declared_fields_instantiate_function_fixed_array_and_self_without_layout() {
    let mut declaration = source_module().structs.remove(0);
    let first = declaration.fields[0].ty.clone();
    let second = declaration.fields[1].ty.clone();
    let owner = applied("Parcel", vec![ValkyrieType::Boolean, ValkyrieType::Utf8]);
    declaration.fields[0].ty = ValkyrieType::Function(Box::new(FunctionType { params: vec![first], return_type: second.clone() }));
    assert_eq!(declaration.instantiate_field(&owner, "head"), Some(ValkyrieType::Function(Box::new(FunctionType {
        params: vec![ValkyrieType::Boolean], return_type: ValkyrieType::Utf8,
    }))));
    declaration.fields[1].ty = ValkyrieType::FixedArray { element: Box::new(second), length: 3 };
    assert_eq!(declaration.instantiate_field(&owner, "tail"), Some(ValkyrieType::FixedArray { element: Box::new(ValkyrieType::Utf8), length: 3 }));
    declaration.fields[2].ty = ValkyrieType::Nullable(Box::new(ValkyrieType::SelfType));
    assert_eq!(declaration.instantiate_field(&owner, "items"), Some(ValkyrieType::Nullable(Box::new(owner))));
}

#[test]
fn source_field_declarations_keep_qualified_owners_and_reject_short_name_aliases() {
    let compiler = ValkyrieCompiler::default();
    let mut declarations: Vec<MirStruct> = Vec::new();
    for namespace in ["first", "second"] {
        let hir = compiler.compile_source(&format!("namespace {namespace}; structure Item {{ value: bool }}"))
            .expect("不同 namespace 的同名声明源码");
        declarations.extend(MirLowerer::lower_module_semantic(&hir).structs);
    }
    assert_eq!(declarations.len(), 2);
    for declaration in &declarations {
        let owner = ValkyrieType::Named(Identifier::new(&declaration.qualified_name()));
        assert_eq!(declaration.instantiate_field(&owner, "value"), Some(ValkyrieType::Boolean));
        assert_eq!(declaration.instantiate_field(&ValkyrieType::Named(Identifier::new("Item")), "value"), None);
    }
    let owner = ValkyrieType::Named(Identifier::new(&declarations[1].qualified_name()));
    assert_eq!(declarations[0].instantiate_field(&owner, "value"), None);
}

#[test]
fn source_field_declaration_reaches_canonical_without_a_construction_seed() {
    let output = ValkyrieCompiler::default().compile_source_to_build_output(
        "structure Flag { value: bool } micro read(flag: Flag) -> bool { return flag.value }",
    ).expect("当前源码字段声明必须贯穿 Compiler 成功载荷");
    let program = output.canonical_program();
    program.validate().expect("Canonical 完整字段身份与结果合同");
    let mut reads = 0;
    for function in program.mir.functions.values() {
        for instruction in function.blocks.values().flat_map(|block| &block.instructions) {
            if let nyar_types::CanonicalOperation::FieldGet { field, .. } = &instruction.operation {
                reads += 1;
                let record = &program.linked.fields[field];
                assert_eq!(record.ty, function.value_types[&instruction.results[0]]);
                assert!(output.compiled_program().representation().adt_reps.contains_key(&record.owner));
            }
        }
    }
    assert_eq!(reads, 1);
}

#[test]
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
fn source_generic_field_write_uses_the_declared_second_argument() {
    let module = ValkyrieCompiler::default().compile_source_to_mir(
        "class Parcel<First, Second> { head: First, tail: Second } \
         micro write(value: Parcel<bool, utf8>, text: utf8) -> unit { value.tail = text }",
    ).expect("泛型字段写入源码必须满足 Semantic MIR 合同");
    validate_semantic_module(&module).expect("完整写入合同");
    let mut invalid = module;
    let function = &mut invalid.functions[0];
    let replacement = function.blocks[0].parameters[0];
    let instruction = function.blocks.iter_mut().flat_map(|block| &mut block.instructions).find(|instruction| {
        matches!(&instruction.kind, MirOperation::FieldSet { .. })
    }).expect("源码必须产生字段写入");
    if let MirOperation::FieldSet { value, .. } = &mut instruction.kind {
        *value = MirOperand::Value(replacement);
    }
    assert_eq!(validate_semantic_module(&invalid).expect_err("写入对象不能冒充声明的 utf8 字段值").code, "SMIR006");
}
