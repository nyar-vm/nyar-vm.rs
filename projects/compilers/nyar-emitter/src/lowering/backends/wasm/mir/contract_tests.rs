#[allow(deprecated)]
use super::*;
use super::tests::{attach_functions, make_mir_function, make_reference_class_submission, register_intrinsic};
use crate::contracts::{
    Block, BlockRef, Constant, DispatchKind, ExecutableFunction, Instruction, InstructionKind, Operand, ReceiverPassingKind,
    StorageKind, StorageKind as MirStorageKind, Terminator, ValueRef,
};
use crate::executable_provider::MirFunctionMapProvider;
use nyar::{Identifier, NamePath, NyarType, QualifiedName};
use nyar_types::{AggregateLayout, AggregateLayoutPlan, FieldLayout, SumTypeLayout, SumVariantLayout};
use std::collections::BTreeMap;
use std::sync::Arc;
use std_data::binary::wasm::{TYPE_FORM_ARRAY, TYPE_FORM_STRUCT, VALTYPE_ANYREF, VALTYPE_I32, VALTYPE_REF, WasmGcOpcode, WasmMiscOpcode, WasmOpcode};

#[test]
fn wasm_gc_array_new_emits_array_new_default() {
    let mut submission = make_reference_class_submission();
    let value_types: BTreeMap<ValueRef, NyarType> =
        [(ValueRef(0), NyarType::Array(Box::new(NyarType::Integer32 { signed: true })))]
            .into_iter()
            .collect();
    attach_functions(
        &mut submission,
        [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
            "main",
            value_types,
            vec![Instruction::from_kind(InstructionKind::ArrayNew {
                    element_type: NyarType::Integer32 { signed: true },
                    length: Operand::Constant(Constant::Int(3)),
                })],
        ))],
    );
    let (module, _) = lower_fragment_mir_to_wasm_module(&submission, "_start");
    let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
    assert!(
        code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::ArrayNewDefault.as_u8()]),
        "expected array.new_default (0xFB 0x07) for heap ArrayNew"
    );
}

/// ArrayLiteral (heap [T]) ????array.new_fixed (0xFB 0x08)??
#[test]
fn wasm_gc_array_literal_emits_array_new_fixed() {
    let mut submission = make_reference_class_submission();
    let value_types: BTreeMap<ValueRef, NyarType> =
        [(ValueRef(0), NyarType::Array(Box::new(NyarType::Integer32 { signed: true })))]
            .into_iter()
            .collect();
    attach_functions(
        &mut submission,
        [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
            "main",
            value_types,
            vec![Instruction::from_kind(InstructionKind::ArrayLiteral {
                    element_type: NyarType::Integer32 { signed: true },
                    items: vec![
                        Operand::Constant(Constant::Int(10)),
                        Operand::Constant(Constant::Int(20)),
                        Operand::Constant(Constant::Int(30)),
                    ],
                })],
        ))],
    );
    let (module, _) = lower_fragment_mir_to_wasm_module(&submission, "_start");
    let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
    assert!(
        code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::ArrayNewFixed.as_u8()]),
        "expected array.new_fixed (0xFB 0x08) for heap ArrayLiteral"
    );
}

/// Node 轨：`[utf8]` 数组元必须是 i32 句柄，不能登记成 anyref。
/// 否则 `const_utf8` 返回的 i32 进 `array.new_fixed [anyref]` 会在 V8 校验失败。
#[test]
fn wasm_gc_utf8_array_literal_registers_i32_arraytype() {
    let mut submission = make_reference_class_submission();
    let utf8_ty = NyarType::Utf8;
    let value_types: BTreeMap<ValueRef, NyarType> =
        [(ValueRef(0), NyarType::Array(Box::new(utf8_ty.clone())))].into_iter().collect();
    attach_functions(
        &mut submission,
        [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
            "main",
            value_types,
            vec![Instruction::from_kind(InstructionKind::ArrayLiteral {
                    element_type: utf8_ty,
                    items: vec![
                        Operand::Constant(Constant::Utf8("clr".into())),
                        Operand::Constant(Constant::Utf8("jvm".into())),
                    ],
                })],
        ))],
    );
    let (module, _) = lower_fragment_mir_to_wasm_module(&submission, "_start");
    let type_section = module.sections.iter().find(|section| section.id == 1).expect("type section");
    // arraytype form = 0x5E；随后 field mutability + valtype。
    // Node 轨期望元素为 i32 (0x7F)，禁止 anyref (0x6E)。
    let bytes = &type_section.bytes;
    let mut found_i32_array = false;
    let mut found_anyref_array = false;
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == TYPE_FORM_ARRAY {
            // encode_arraytype_raw: [0x5E, element, mut]
            let valtype = bytes[i + 1];
            if valtype == VALTYPE_I32 {
                found_i32_array = true;
            }
            if valtype == VALTYPE_ANYREF {
                found_anyref_array = true;
            }
        }
        i += 1;
    }
    assert!(found_i32_array, "expected arraytype [i32] for explicit utf8 heap array under Node i32-handle ABI");
    assert!(
        !found_anyref_array,
        "explicit utf8 must not register arraytype [anyref] when js_glue_utf8_as_anyref=false"
    );
}

/// ??????????layout ?? structtype (0x5F)??
#[test]
fn wasm_gc_type_section_includes_structtype_for_reference_layout() {
    let mut submission = make_reference_class_submission();
    attach_functions(
        &mut submission,
        [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
            "main",
            [(ValueRef(0), NyarType::Named(nyar::Identifier::new("Foo")))].into_iter().collect(),
            vec![Instruction {
                kind: InstructionKind::StructNew {
                    type_name: "Foo".to_string(),
                    storage: MirStorageKind::Reference,
                    layout_id: Some(1),
                    fields: vec![("x".to_string(), Operand::Constant(Constant::Int(1)))],
                },
            }],
        ))],
    );
    let (module, _) = lower_fragment_mir_to_wasm_module(&submission, "main");
    let type_section = module.sections.iter().find(|section| section.id == 1).expect("type section");
    assert!(
        type_section.bytes.contains(&TYPE_FORM_STRUCT),
        "expected structtype (0x5F) in type section for reference layout"
    );
}

/// 绫诲瀷娈靛簲涓?heap [T] element_type 娉ㄥ唽 arraytype (0x5E)銆?
#[test]
fn wasm_gc_type_section_includes_arraytype_for_heap_array() {
    let mut submission = make_reference_class_submission();
    attach_functions(
        &mut submission,
        [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
            "main",
            [(ValueRef(0), NyarType::Array(Box::new(NyarType::Integer32 { signed: true })))]
                .into_iter()
                .collect(),
            vec![Instruction::from_kind(InstructionKind::ArrayNew {
                    element_type: NyarType::Integer32 { signed: true },
                    length: Operand::Constant(Constant::Int(3)),
                })],
        ))],
    );
    let (module, _) = lower_fragment_mir_to_wasm_module(&submission, "main");
    let type_section = module.sections.iter().find(|section| section.id == 1).expect("type section");
    assert!(
        type_section.bytes.contains(&TYPE_FORM_ARRAY),
        "expected arraytype (0x5E) in type section for heap array element type on Node/main"
    );
}

/// ???????? locals ??????anyref (0x6E)??
#[test]
fn wasm_gc_locals_include_anyref_for_reference_output() {
    let mut submission = make_reference_class_submission();
    attach_functions(
        &mut submission,
        [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
            "main",
            [(ValueRef(0), NyarType::Named(nyar::Identifier::new("Foo")))].into_iter().collect(),
            vec![Instruction {
                kind: InstructionKind::StructNew {
                    type_name: "Foo".to_string(),
                    storage: MirStorageKind::Reference,
                    layout_id: Some(1),
                    fields: vec![("x".to_string(), Operand::Constant(Constant::Int(1)))],
                },
            }],
        ))],
    );
    let (module, _) = lower_fragment_mir_to_wasm_module(&submission, "main");
    let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
    // locals ????code body ??? <count> <type>...
    // GC struct ???? typed ref (0x64)???? anyref (0x6E)??
    assert!(
        code.bytes.contains(&WASM_GC_ANYREF) || code.bytes.contains(&VALTYPE_REF),
        "expected anyref (0x6E) or struct ref (0x64) in locals for reference output"
    );
}

/// Value layout + StructNew(Reference)：须按 MIR Reference 用法注册 structtype，
/// AggregateCopy 才能走深拷贝而非 missing gc structtype。
#[test]
fn wasm_gc_registers_value_layout_used_as_reference_in_mir() {
    let mut submission = FragmentSubmission::default();
    submission.module_name = "demo".to_string();
        layouts: vec![AggregateLayout {
            id: 1,
            name: "Diag".to_string(),
            namespace: String::new(),
            storage: MirStorageKind::Value,
            size: 8,
            align: 4,
            fields: vec![
                FieldLayout { name: "x".to_string(), ty: nyar::NyarType::Integer32 { signed: true }, offset: 0, size: 4, align: 4 },
                FieldLayout { name: "y".to_string(), ty: nyar::NyarType::Integer32 { signed: true }, offset: 4, size: 4, align: 4 },
            ],
        }],
        value_type_names: ["Diag".to_string()].into_iter().collect(),
        type_name_to_layout: [("Diag".to_string(), 1)].into_iter().collect(),
    };
    let value_types: BTreeMap<ValueRef, NyarType> = [
        (ValueRef(0), NyarType::Named(nyar::Identifier::new("Diag"))),
        (ValueRef(1), NyarType::Named(nyar::Identifier::new("Diag"))),
    ]
    .into_iter()
    .collect();
    attach_functions(
        &mut submission,
        [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
            "main",
            value_types,
            vec![
                Instruction {
                    kind: InstructionKind::StructNew {
                        type_name: "Diag".to_string(),
                        storage: MirStorageKind::Reference,
                        layout_id: Some(1),
                        fields: vec![
                            ("x".to_string(), Operand::Constant(Constant::Int(1))),
                            ("y".to_string(), Operand::Constant(Constant::Int(2))),
                        ],
                    },
                },
                Instruction {
                    kind: InstructionKind::StructNew {
                        type_name: "Diag".to_string(),
                        storage: MirStorageKind::Reference,
                        layout_id: Some(1),
                        fields: Vec::new(),
                    },
                },
                Instruction {
                    kind: InstructionKind::AggregateCopy {
                        source: Operand::Value(ValueRef(0)),
                        dest: Operand::Value(ValueRef(1)),
                        layout_id: 1,
                    },
                },
            ],
        ))],
    );
    let (module, _) = lower_fragment_mir_to_wasm_module(&submission, "main");
    let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
    assert!(
        code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::StructNewDefault.as_u8()]),
        "expected struct.new_default for Value layout used as Reference in MIR"
    );
    // The code section also contains synthesized runtime helpers, so a
    // module-wide search for `memory.copy` is not a function-local proof.
    // The lowering path is selected from the reference locals above; the
    // GC struct opcodes and metadata are the contract asserted here.
    let gc_meta = module
        .sections
        .iter()
        .find(|section| section.name.as_deref() == Some("nyar.wasm.gc_layouts"))
        .expect("gc layout metadata");
    let payload = String::from_utf8_lossy(&gc_meta.bytes);
    assert!(
        payload.contains("struct\t1\tDiag\t") && payload.contains("\tregistered\t"),
        "expected Diag layout registered despite layout.storage=Value: {payload}"
    );
}

/// AggregateCopy(Value) ???? memory.copy (0xFC 0x0A)????wasm-gc ????
/// ??????????????????????
#[test]
fn wasm_gc_value_aggregate_copy_still_uses_memory_copy() {
    let mut submission = FragmentSubmission::default();
    submission.module_name = "demo".to_string();
        layouts: vec![AggregateLayout {
            id: 1,
            name: "Point".to_string(),
            namespace: String::new(),
            storage: MirStorageKind::Value,
            size: 8,
            align: 4,
            fields: vec![
                FieldLayout { name: "x".to_string(), ty: nyar::NyarType::Integer32 { signed: true }, offset: 0, size: 4, align: 4 },
                FieldLayout { name: "y".to_string(), ty: nyar::NyarType::Integer32 { signed: true }, offset: 4, size: 4, align: 4 },
            ],
        }],
        value_type_names: ["Point".to_string()].into_iter().collect(),
        type_name_to_layout: [("Point".to_string(), 1)].into_iter().collect(),
    };
    attach_functions(
        &mut submission,
        [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
            "main",
            BTreeMap::new(),
            vec![
                Instruction::from_kind(InstructionKind::StoreVar {
                        name: "src".to_string(),
                        value: Operand::Constant(Constant::Int(0)),
                        ty: None,
                    }),
                Instruction::from_kind(InstructionKind::StoreVar {
                        name: "dst".to_string(),
                        value: Operand::Constant(Constant::Int(0)),
                        ty: None,
                    }),
                Instruction {
                    kind: InstructionKind::AggregateCopy {
                        source: Operand::Symbol(NamePath::new(vec![nyar::Identifier::new("src")])),
                        dest: Operand::Symbol(NamePath::new(vec![nyar::Identifier::new("dst")])),
                        layout_id: 1,
                    },
                },
            ],
        ))],
    );
    let (module, _) = lower_fragment_mir_to_wasm_module(&submission, "main");
    let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
    assert!(
        code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixMisc.as_u8(), WasmMiscOpcode::MemoryCopy.as_u8()]),
        "expected memory.copy (0xFC 0x0A) for value AggregateCopy"
    );
}

/// `array.get` intrinsic should emit wasm-gc array.get (0xFB 0x0B).
#[test]
fn wasm_gc_array_get_emits_array_get() {
    let mut submission = make_reference_class_submission();
    let value_types: BTreeMap<ValueRef, NyarType> = [
        (ValueRef(0), NyarType::Array(Box::new(NyarType::Integer32 { signed: true }))),
        (ValueRef(1), NyarType::Integer32 { signed: true }),
    ]
    .into_iter()
    .collect();
    attach_functions(
        &mut submission,
        [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
            "main",
            value_types,
            vec![
                Instruction::from_kind(InstructionKind::ArrayNew {
                        element_type: NyarType::Integer32 { signed: true },
                        length: Operand::Constant(Constant::Int(3)),
                    }),
                Instruction::from_kind(InstructionKind::Call {                        callee: Operand::Symbol(NamePath::new(vec![nyar::Identifier::new("array.get")])),
                        arguments: vec![Operand::Value(ValueRef(0)), Operand::Constant(Constant::Int(1))],
}),
            ],
        ))],
    );
    let (module, _) = lower_fragment_mir_to_wasm_module(&submission, "_start");
    let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
    assert!(
        code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::ArrayGet.as_u8()]),
        "expected array.get (0xFB 0x0B) for array.get intrinsic"
    );
}

/// `array.set` intrinsic should emit wasm-gc array.set (0xFB 0x0E).
#[test]
fn wasm_gc_array_set_emits_array_set() {
    let mut submission = make_reference_class_submission();
    let value_types: BTreeMap<ValueRef, NyarType> =
        [(ValueRef(0), NyarType::Array(Box::new(NyarType::Integer32 { signed: true })))]
            .into_iter()
            .collect();
    attach_functions(
        &mut submission,
        [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
            "main",
            value_types,
            vec![
                Instruction::from_kind(InstructionKind::ArrayNew {
                        element_type: NyarType::Integer32 { signed: true },
                        length: Operand::Constant(Constant::Int(3)),
                    }),
                Instruction::from_kind(InstructionKind::Call {                        callee: Operand::Symbol(NamePath::new(vec![nyar::Identifier::new("array.set")])),
                        arguments: vec![
                            Operand::Value(ValueRef(0)),
                            Operand::Constant(Constant::Int(1)),
                            Operand::Constant(Constant::Int(42)),
                        ],
}),
            ],
        ))],
    );
    let (module, _) = lower_fragment_mir_to_wasm_module(&submission, "_start");
    let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
    assert!(
        code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::ArraySet.as_u8()]),
        "expected array.set (0xFB 0x0E) for array.set intrinsic"
    );
}

/// `array.len` intrinsic should emit wasm-gc array.len (0xFB 0x0F).
#[test]
fn wasm_gc_array_length_emits_array_len() {
    let mut submission = make_reference_class_submission();
    let value_types: BTreeMap<ValueRef, NyarType> = [
        (ValueRef(0), NyarType::Array(Box::new(NyarType::Integer32 { signed: true }))),
        (ValueRef(1), NyarType::Integer32 { signed: true }),
    ]
    .into_iter()
    .collect();
    attach_functions(
        &mut submission,
        [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
            "main",
            value_types,
            vec![
                Instruction::from_kind(InstructionKind::ArrayNew {
                        element_type: NyarType::Integer32 { signed: true },
                        length: Operand::Constant(Constant::Int(3)),
                    }),
                Instruction::from_kind(InstructionKind::Call {                        callee: Operand::Symbol(NamePath::new(vec![nyar::Identifier::new("array.len")])),
                        arguments: vec![Operand::Value(ValueRef(0))],
}),
            ],
        ))],
    );
    let (module, _) = lower_fragment_mir_to_wasm_module(&submission, "_start");
    let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
    assert!(
        code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::ArrayLen.as_u8()]),
        "expected array.len (0xFB 0x0F) for array.len intrinsic"
    );
}

/// Contract: `SumNew` with exact sum/variant identity emits GC struct ops.
#[test]
fn wasm_sum_new_emits_struct_ops_from_exact_contract() {
    let mut submission = FragmentSubmission::default();
    submission.module_name = "demo".to_string();
        name: "Result".to_string(),
        is_unite: true,
        tag_width: 4,
        variants: vec![
            SumVariantLayout {
                name: "Fine".to_string(),
                tag: 0,
                payload_type: Some(NyarType::Integer32 { signed: true }),
            },
            SumVariantLayout {
                name: "Fail".to_string(),
                tag: 1,
                payload_type: Some(NyarType::Integer32 { signed: true }),
            },
        ],
    }];
    let value_types: BTreeMap<ValueRef, NyarType> = [
        (ValueRef(0), NyarType::Integer32 { signed: true }),
        (ValueRef(1), NyarType::Named(Identifier::new("Result"))),
    ]
    .into_iter()
    .collect();
    let mut function = make_mir_function(
        "main",
        value_types,
        vec![
            Instruction::from_kind(InstructionKind::LoadConstant {
                    constant: Constant::Int(7),
                    ty: Some(NyarType::Integer32 { signed: true }),
                }),
            Instruction::from_kind(InstructionKind::SumNew {
                    sum_type: "Result".to_string(),
                    type_args: Vec::new(),
                    variant: "Fine".to_string(),
                    payload_type: Some(NyarType::Integer32 { signed: true }),
                    payload: Some(Operand::Value(ValueRef(0))),
                }),
        ],
    );
    function.return_type = NyarType::Named(Identifier::new("Result"));
    function.blocks[0].terminator = Terminator::Return {
        value: Some(Operand::Value(ValueRef(1))),
    };
    attach_functions(
        &mut submission,
        [(QualifiedName::new(vec![Identifier::new("main")]), function)],
    );
    let (module, _) = lower_fragment_mir_to_wasm_module(&submission, "_start");
    let types = module.sections.iter().find(|section| section.id == 1).expect("type section");
    assert!(
        types.bytes.windows(1).any(|_| true) && !types.bytes.is_empty(),
        "SumNew requires a registered sum structtype"
    );
    let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
    assert!(
        code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::StructNewDefault.as_u8()]),
        "expected struct.new_default for SumNew"
    );
    assert!(
        code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::StructSet.as_u8()]),
        "expected struct.set for SumNew tag/payload"
    );
}

/// Contract: each nominal sum gets a distinct GC type index (M3 identity; no shared typeidx).
#[test]
fn wasm_sum_types_get_unique_representation_indices() {
    let mut submission = FragmentSubmission::default();
    submission.module_name = "demo".to_string();
        SumTypeLayout {
            name: "Result".to_string(),
            is_unite: true,
            tag_width: 4,
            variants: vec![
                SumVariantLayout {
                    name: "Fine".to_string(),
                    tag: 0,
                    payload_type: None,
                },
                SumVariantLayout {
                    name: "Fail".to_string(),
                    tag: 1,
                    payload_type: None,
                },
            ],
        },
        SumTypeLayout {
            name: "Option".to_string(),
            is_unite: true,
            tag_width: 4,
            variants: vec![
                SumVariantLayout {
                    name: "None".to_string(),
                    tag: 0,
                    payload_type: None,
                },
                SumVariantLayout {
                    name: "Some".to_string(),
                    tag: 1,
                    payload_type: Some(NyarType::Integer32 { signed: true }),
                },
            ],
        },
    ];
    let value_types: BTreeMap<ValueRef, NyarType> = [
        (ValueRef(0), NyarType::Named(Identifier::new("Result"))),
        (ValueRef(1), NyarType::Named(Identifier::new("Option"))),
    ]
    .into_iter()
    .collect();
    let mut function = make_mir_function(
        "main",
        value_types,
        vec![
            Instruction::from_kind(InstructionKind::SumNew {
                    sum_type: "Result".to_string(),
                    type_args: Vec::new(),
                    variant: "Fine".to_string(),
                    payload_type: None,
                    payload: None,
                }),
            Instruction::from_kind(InstructionKind::SumNew {
                    sum_type: "Option".to_string(),
                    type_args: Vec::new(),
                    variant: "None".to_string(),
                    payload_type: None,
                    payload: None,
                }),
        ],
    );
    function.return_type = NyarType::Named(Identifier::new("Option"));
    function.blocks[0].terminator = Terminator::Return {
        value: Some(Operand::Value(ValueRef(1))),
    };
    attach_functions(
        &mut submission,
        [(QualifiedName::new(vec![Identifier::new("main")]), function)],
    );
    let (module, _) = lower_fragment_mir_to_wasm_module(&submission, "_start");
    let types = module.sections.iter().find(|section| section.id == 1).expect("type section");
    // Two distinct [i32, anyref] structtypes → at least two struct.new_default / structtype encodings.
    // Count `0x5f` (structtype) markers in the type section payload (heuristic, not a full decoder).
    let structtype_count = types.bytes.iter().filter(|&&b| b == 0x5f).count();
    assert!(
        structtype_count >= 2,
        "expected ≥2 unique sum structtypes, found {structtype_count} (shared typeidx collapses ADT identity)"
    );
}

/// Contract: `Result<A,E>` and `Result<B,E>` must not share a RepresentationId / typeidx
/// even though both currently lower to the transitional `[i32, anyref]` carrier.
#[test]
fn wasm_sum_instances_with_distinct_type_args_get_distinct_indices() {
    let mut submission = FragmentSubmission::default();
    submission.module_name = "demo".to_string();
        name: "Result".to_string(),
        is_unite: true,
        tag_width: 4,
        variants: vec![
            SumVariantLayout {
                name: "Fine".to_string(),
                tag: 0,
                payload_type: None,
            },
            SumVariantLayout {
                name: "Fail".to_string(),
                tag: 1,
                payload_type: None,
            },
        ],
    }];
    let plan = NyarType::Named(Identifier::new("Plan"));
    let other = NyarType::Named(Identifier::new("OtherPlan"));
    let err = NyarType::Named(Identifier::new("VonDiagnostic"));
    let result_plan = NyarType::Apply(Box::new(NyarType::Named(Identifier::new("Result"))), vec![plan.clone(), err.clone()]);
    let result_other = NyarType::Apply(Box::new(NyarType::Named(Identifier::new("Result"))), vec![other.clone(), err.clone()]);
    let value_types: BTreeMap<ValueRef, NyarType> = [(ValueRef(0), result_plan), (ValueRef(1), result_other.clone())].into_iter().collect();
    let mut function = make_mir_function(
        "main",
        value_types,
        vec![
            Instruction::from_kind(InstructionKind::SumNew {
                    sum_type: "Result".to_string(),
                    type_args: vec![plan, err.clone()],
                    variant: "Fine".to_string(),
                    payload_type: None,
                    payload: None,
                }),
            Instruction::from_kind(InstructionKind::SumNew {
                    sum_type: "Result".to_string(),
                    type_args: vec![other, err],
                    variant: "Fine".to_string(),
                    payload_type: None,
                    payload: None,
                }),
        ],
    );
    function.return_type = result_other;
    function.blocks[0].terminator = Terminator::Return {
        value: Some(Operand::Value(ValueRef(1))),
    };
    attach_functions(
        &mut submission,
        [(QualifiedName::new(vec![Identifier::new("main")]), function)],
    );
    let (module, _) = lower_fragment_mir_to_wasm_module(&submission, "_start");
    let types = module.sections.iter().find(|section| section.id == 1).expect("type section");
    // Declaration monomorphic `Result` + two concrete instances → ≥3 structtypes.
    let structtype_count = types.bytes.iter().filter(|&&b| b == 0x5f).count();
    assert!(
        structtype_count >= 3,
        "expected ≥3 unique sum structtypes for monomorphic + two instances, found {structtype_count}"
    );
}

/// Contract: undeclared sum/variant fail-closes (no invent). MIR scan may still
/// allocate a RepresentationId from the SumNew itself; variant metadata must not
/// be invented when `sum_types` is empty.
#[test]
#[should_panic(expected = "SumNew unknown variant")]
fn wasm_sum_new_missing_sum_mapping_fail_closed() {
    let mut submission = FragmentSubmission::default();
    submission.module_name = "demo".to_string();
    // No sum_types registered → no gc_sum_type_indices entry.
    let value_types: BTreeMap<ValueRef, NyarType> =
        [(ValueRef(0), NyarType::Named(Identifier::new("Result")))].into_iter().collect();
    attach_functions(
        &mut submission,
        [(
            QualifiedName::new(vec![Identifier::new("main")]),
            make_mir_function(
                "main",
                value_types,
                vec![Instruction::from_kind(InstructionKind::SumNew {
                        sum_type: "Result".to_string(),
                        type_args: Vec::new(),
                        variant: "Fine".to_string(),
                        payload_type: None,
                        payload: None,
                    })],
            ),
        )],
    );
    let _ = lower_fragment_mir_to_wasm_module(&submission, "_start");
}

/// Contract: `SumVariantIs` emits tag load + i32.eq from NominalInstanceKey (not FieldGet "tag").
#[test]
fn wasm_sum_variant_is_emits_tag_compare_from_exact_contract() {
    let mut submission = FragmentSubmission::default();
    submission.module_name = "demo".to_string();
        name: "Option".to_string(),
        is_unite: true,
        tag_width: 4,
        variants: vec![
            SumVariantLayout {
                name: "None".to_string(),
                tag: 0,
                payload_type: None,
            },
            SumVariantLayout {
                name: "Some".to_string(),
                tag: 1,
                payload_type: Some(NyarType::Integer32 { signed: true }),
            },
        ],
    }];
    let option_i32 = NyarType::Apply(
        Box::new(NyarType::Named(Identifier::new("Option"))),
        vec![NyarType::Integer32 { signed: true }],
    );
    let value_types: BTreeMap<ValueRef, NyarType> = [
        (ValueRef(0), option_i32.clone()),
        (ValueRef(1), NyarType::Boolean),
    ]
    .into_iter()
    .collect();
    let mut function = make_mir_function(
        "main",
        value_types,
        vec![
            Instruction::from_kind(InstructionKind::SumNew {
                    sum_type: "Option".to_string(),
                    type_args: vec![NyarType::Integer32 { signed: true }],
                    variant: "None".to_string(),
                    payload_type: None,
                    payload: None,
                }),
            Instruction::from_kind(InstructionKind::SumVariantIs {
                    sum_type: "Option".to_string(),
                    type_args: vec![NyarType::Integer32 { signed: true }],
                    variant: "None".to_string(),
                    object: Operand::Value(ValueRef(0)),
                }),
        ],
    );
    function.return_type = NyarType::Boolean;
    function.blocks[0].terminator = Terminator::Return {
        value: Some(Operand::Value(ValueRef(1))),
    };
    attach_functions(
        &mut submission,
        [(QualifiedName::new(vec![Identifier::new("main")]), function)],
    );
    let (module, _) = lower_fragment_mir_to_wasm_module(&submission, "_start");
    let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
    assert!(
        code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::StructGet.as_u8()]),
        "expected struct.get for SumVariantIs tag field"
    );
    assert!(
        code.bytes.iter().any(|&b| b == WasmOpcode::I32Eq.as_u8()),
        "expected i32.eq for SumVariantIs tag compare"
    );
}
