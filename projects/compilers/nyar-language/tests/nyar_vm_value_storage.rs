//! NyarVM value-storage lowering coverage that needs the language frontend.
//!
//! Lives here (not in `emitter` unit tests) so the driver crate stays free of
//! `nyar-language` while still covering compiler → executable → VM bytecode.
//!
//! 使用正式 `nyar-vm-lane` feature 验证字节码值存储合同。

use std::sync::Arc;

use emitter::{FragmentSubmission, executable_provider::MirFunctionMapProvider, testing};
use nyar::Identifier;
use nyar_language::{MirLowerer, ValkyrieCompiler, mir_function_to_executable, types::SourceID};
use nyar_bytecode::{NyarHeadCode, NyarModuleData};
use nyar_vm::{NyarVm, value::Value};

/// 模块 imports 表是否声明指定宿主符号（`nyar.host`）。
fn module_declares_host_import(module: &NyarModuleData, name: &str) -> bool {
    module.imports.iter().any(|import| import.module_name == "nyar.host" && import.symbol_name == name)
}

/// 字节码是否含给定单字节操作码。
fn module_contains_opcode(module: &NyarModuleData, opcode: NyarHeadCode) -> bool {
    module.code_bytes.contains(&(opcode as u8))
}

fn lower_main_from_source(source: &str, version_id: u32) -> NyarModuleData {
    let hir = ValkyrieCompiler::new(SourceID { version_id }).compile_source(source).expect("compile");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let plan = mir.aggregate_layouts.clone();
    let main_symbol = mir.functions.iter().find(|function| function.symbol.ends_with("main")).expect("main mir");
    let operation = nyar::QualifiedName::new(vec![Identifier::new("main")]);
    let mut submission = FragmentSubmission::default();
    submission.aggregate_layouts = plan;
    submission.exported_operations = vec![operation.clone()];
    submission.executable =
        Some(Arc::new(MirFunctionMapProvider::new(
            [(operation, mir_function_to_executable(main_symbol, &mir.sum_types).expect("完整 MIR 模块必须提供 sum registry"))]
                .into_iter()
                .collect(),
        )));
    testing::lower_fragment_to_nyar_module(&submission).expect("Compiler-owned executable lowering")
}

#[test]
fn nyar_vm_struct_new_value_storage_lowers_to_object_new() {
    let module = lower_main_from_source(
        r#"
structure Point {
    x: f64,
    y: f64,
}

micro main() {
    Point { x: 1.0, y: 2.0 }
}
"#,
        9700,
    );
    // StructNew → ObjectNew(layout_id) + FieldSet(slot)；不再经字符串宿主 alloc_record。
    assert!(!module.layouts.is_empty(), "StructNew should populate layouts section");
    assert!(module_contains_opcode(&module, NyarHeadCode::ObjectNew), "bytecode should contain ObjectNew");
    assert!(module_contains_opcode(&module, NyarHeadCode::FieldSet), "bytecode should contain FieldSet");
    assert!(!module_declares_host_import(&module, "alloc_record"), "StructNew must not declare alloc_record");
    assert!(!module_declares_host_import(&module, "record_set"), "StructNew must not declare record_set");
}

#[test]
fn nyar_vm_aggregate_copy_lowers_to_field_ops() {
    let module = lower_main_from_source(
        r#"
structure Point {
    x: f64,
    y: f64,
}

micro main() {
    let p1 = Point { x: 1.0, y: 2.0 };
    let p2 = p1;
    p2
}
"#,
        9701,
    );
    assert!(!module.layouts.is_empty(), "AggregateCopy should populate layouts section");
    assert!(module_contains_opcode(&module, NyarHeadCode::ObjectNew), "bytecode should contain ObjectNew");
    assert!(module_contains_opcode(&module, NyarHeadCode::FieldGet), "bytecode should contain FieldGet");
    assert!(module_contains_opcode(&module, NyarHeadCode::FieldSet), "bytecode should contain FieldSet");
    assert!(!module_declares_host_import(&module, "alloc_record"), "AggregateCopy must not declare alloc_record");
    assert!(!module_declares_host_import(&module, "record_get"), "AggregateCopy must not declare record_get");
    assert!(!module_declares_host_import(&module, "record_set"), "AggregateCopy must not declare record_set");
}

#[test]
fn nyar_vm_field_get_value_path_uses_field_get() {
    let module = lower_main_from_source(
        r#"
structure Point {
    x: f64,
    y: f64,
}

micro main() {
    let p1 = Point { x: 1.0, y: 2.0 };
    p1.x
}
"#,
        9702,
    );
    assert!(module_contains_opcode(&module, NyarHeadCode::FieldGet), "bytecode should contain FieldGet");
    assert!(!module_declares_host_import(&module, "record_get"), "FieldGet must not declare record_get");
}

#[test]
fn nyar_vm_loop_preserves_entry_arguments() {
    let module = lower_main_from_source(
        r#"
micro main(first: i64, second: i64) -> i64 {
    let mut current = second;
    let mut count = 0;
    while count < 3 {
        current = current + 1;
        count = count + 1;
    }
    return first;
}
"#,
        9710,
    );
    let bytes = nyar_bytecode::encode_module(&module);
    let mut vm = NyarVm::new();
    let loaded = vm.load(&bytes).expect("verify and load");
    let result = vm.run(&loaded, "main", vec![Value::I32(41), Value::I32(7)]).expect("execute");
    assert_eq!(result, Value::I32(41));
}

#[test]
fn nyar_vm_loop_copies_parameters_in_parallel() {
    let module = lower_main_from_source(
        r#"
micro main(first: i64, second: i64) -> i64 {
    let mut left = first;
    let mut right = second;
    let mut count = 0;
    while count < 3 {
        let saved = left;
        left = right;
        right = saved;
        count = count + 1;
    }
    return left * 10 + right;
}
"#,
        9711,
    );
    let bytes = nyar_bytecode::encode_module(&module);
    let mut vm = NyarVm::new();
    let loaded = vm.load(&bytes).expect("verify and load");
    let result = vm.run(&loaded, "main", vec![Value::I32(2), Value::I32(7)]).expect("execute");
    assert_eq!(result, Value::I64(72));
}

#[test]
fn nyar_vm_assignment_does_not_rebind_source_value() {
    let module = lower_main_from_source(
        r#"
micro main(first: i64, second: i64) -> i64 {
    let mut current = first;
    current = second;
    return first;
}
"#,
        9712,
    );
    let bytes = nyar_bytecode::encode_module(&module);
    let mut vm = NyarVm::new();
    let loaded = vm.load(&bytes).expect("verify and load");
    let result = vm.run(&loaded, "main", vec![Value::I32(41), Value::I32(7)]).expect("execute");
    assert_eq!(result, Value::I32(41));
}
