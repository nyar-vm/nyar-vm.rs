//! 泛型 `StructNew`：字段类型须在同一 substitution 下与布局声明对齐（SMIR010）。

use std::{collections::BTreeMap, sync::Arc};

use nyar::{Identifier, QualifiedName};
use nyar_emitter::{FragmentSubmission, executable_provider::MirFunctionMapProvider, testing::semantic_mir_observation};
use nyar_language::{SourceID, ValkyrieCompiler, mir::ssa::MirLowerer};

fn qualified_symbol_from_string(symbol: &str) -> QualifiedName {
    QualifiedName::new(symbol.split([':', '.']).filter(|part| !part.is_empty()).map(Identifier::new).collect::<Vec<_>>())
}

#[test]
fn generic_box_new_passes_struct_new_contract_under_substitution() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9601 })
        .compile_source(
            r#"
class Box<T> {
    _items: [T]
    _cap: usize
}

imply Box<T> {
    micro new(cap: usize): Self {
        return Box { _items: [], _cap: cap }
    }
}
"#,
        )
        .expect("compile Box");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let new_fn = mir.functions.iter().find(|function| function.symbol.ends_with("new")).expect("Box.new");
    let operation = qualified_symbol_from_string(&new_fn.symbol);
    let mut mir_map = BTreeMap::new();
    mir_map.insert(operation.clone(), new_fn.clone().into());

    let mut submission = FragmentSubmission::default();
    submission.module_name = "box_new_contract".to_string();
    submission.aggregate_layouts = mir.aggregate_layouts.clone();
    submission.exported_operations = vec![operation];
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(mir_map)));

    let observation = semantic_mir_observation(&submission, "generic_box_new");
    assert_eq!(observation, "generic_box_new|accept||", "SMIR010 must accept Box.new under type-arg substitution; got {observation}");
}

#[test]
fn array_list_new_passes_struct_new_contract_under_substitution() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9602 })
        .compile_source(
            r#"
class ArrayList<T> {
    _items: [T]
    _capacity: usize
}

imply ArrayList<T> {
    micro new(capacity: usize): Self {
        return ArrayList { _items: [], _capacity: capacity }
    }
}
"#,
        )
        .expect("compile ArrayList");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let new_fn = mir.functions.iter().find(|function| function.symbol.ends_with("new")).expect("ArrayList.new");
    let operation = qualified_symbol_from_string(&new_fn.symbol);
    let mut mir_map = BTreeMap::new();
    mir_map.insert(operation.clone(), new_fn.clone().into());

    let mut submission = FragmentSubmission::default();
    submission.module_name = "array_list_new_contract".to_string();
    submission.aggregate_layouts = mir.aggregate_layouts.clone();
    submission.exported_operations = vec![operation];
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(mir_map)));

    let observation = semantic_mir_observation(&submission, "array_list_new");
    assert_eq!(observation, "array_list_new|accept||", "SMIR010 must accept ArrayList.new under type-arg substitution; got {observation}");
}

#[test]
fn named_self_output_rejects_without_string_special_case() {
    use nyar_types::{
        Block, BlockRef, Instruction, InstructionKind, NyarType, Operand, Terminator, ValueRef,
        layout::{AggregateLayout, AggregateLayoutPlan, FieldLayout, StorageKind},
    };

    let output = ValueRef(0);
    let items = ValueRef(1);
    let cap = ValueRef(2);
    let mut value_types = BTreeMap::new();
    // 故意留下 Named("Self") 污点：合同必须失败关闭，不得按符号猜 owner。
    value_types.insert(output, NyarType::Named(Identifier::new("Self")));
    value_types.insert(items, NyarType::Array(Box::new(NyarType::Named(Identifier::new("T")))));
    value_types.insert(cap, NyarType::Integer32 { signed: true });

    let mut struct_new = Instruction::from_kind(InstructionKind::StructNew {
        type_name: "Box".to_string(),
        fields: vec![("_items".to_string(), Operand::Value(items)), ("_cap".to_string(), Operand::Value(cap))],
    });
    struct_new.results = vec![output];

    let function = nyar_emitter::executable_provider::ExecutableFunction {
        symbol: "Box.new".to_string(),
        return_type: NyarType::Named(Identifier::new("Self")),
        param_types: vec![NyarType::Integer32 { signed: true }],
        value_types,
        entry: BlockRef(0),
        values: Vec::new(),
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![struct_new],
            terminator: Terminator::Return { value: Some(Operand::Value(output)) },
        }],
        diagnostics: Vec::new(),
    };

    let mut layouts = AggregateLayoutPlan::default();
    layouts.layouts.push(AggregateLayout {
        id: 1,
        name: "Box".to_string(),
        namespace: String::new(),
        storage: StorageKind::Reference,
        size: 16,
        align: 8,
        fields: vec![
            FieldLayout {
                name: "_items".to_string(),
                ty: NyarType::Array(Box::new(NyarType::Named(Identifier::new("T")))),
                offset: 0,
                size: 8,
                align: 8,
            },
            FieldLayout { name: "_cap".to_string(), ty: NyarType::Integer32 { signed: true }, offset: 8, size: 4, align: 4 },
        ],
    });

    let operation = QualifiedName::new(vec![Identifier::new("Box"), Identifier::new("new")]);
    let mut mir_map = BTreeMap::new();
    mir_map.insert(operation.clone(), function);

    let mut submission = FragmentSubmission::default();
    submission.aggregate_layouts = layouts;
    submission.exported_operations = vec![operation];
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(mir_map)));

    let observation = semantic_mir_observation(&submission, "named_self_output");
    assert!(
        observation.contains("|reject|SMIR010|"),
        "Named(\"Self\") output must fail closed without symbol-name recovery; got {observation}"
    );
}
