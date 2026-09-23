//! NyarVM value-storage lowering coverage that needs the language frontend.
//!
//! Lives here (not in `emitter` unit tests) so the driver crate stays free of
//! `nyar-language` while still covering compiler → executable → VM bytecode.

use std::sync::Arc;

use emitter::{FragmentSubmission, executable_provider::MirFunctionMapProvider, testing};
use nyar::Identifier;
use nyar_language::{MirLowerer, ValkyrieCompiler, mir_function_to_executable, types::SourceID};

/// 检查 module 的常量池中是否存在指定的 native call 名称字符串。
fn module_has_native_call(module: &std_data::binary::nyar_ir::NyarModuleData, name: &str) -> bool {
    module.constants.iter().any(|constant| matches!(constant, std_data::binary::nyar_ir::NyarConstant::String(text) if text == name))
}

/// 检查 module 的字节码中是否包含 CallNative 指令调用指定名称。
fn module_invokes_native(module: &std_data::binary::nyar_ir::NyarModuleData, name: &str) -> bool {
    let Some(name_index) =
        module.constants.iter().position(|constant| matches!(constant, std_data::binary::nyar_ir::NyarConstant::String(text) if text == name))
    else {
        return false;
    };
    let name_index = name_index as i32;
    let needle = name_index.to_le_bytes();
    module.code_bytes.windows(9).any(|window| window[0] == std_data::binary::nyar_ir::NyarHeadCode::CallNative as u8 && window[1..5] == needle)
}

fn lower_main_from_source(source: &str, version_id: u32) -> std_data::binary::nyar_ir::NyarModuleData {
    let hir = ValkyrieCompiler::new(SourceID { version_id }).compile_source(source).expect("compile");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let plan = mir.aggregate_layouts.clone();
    let main_symbol = mir.functions.iter().find(|function| function.symbol.ends_with("main")).expect("main mir");
    let operation = nyar::QualifiedName::new(vec![Identifier::new("main")]);
    let mut submission = FragmentSubmission::default();
    submission.aggregate_layouts = plan;
    submission.executable =
        Some(Arc::new(MirFunctionMapProvider::new([(operation, mir_function_to_executable(main_symbol))].into_iter().collect())));
    testing::lower_fragment_to_nyar_module(&submission)
}

#[test]
fn nyar_vm_struct_new_value_storage_lowers_to_record() {
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
    // StructNew Value 路径必须经 alloc_record 构造新 Record，再逐字段 record_set 写入。
    assert!(module_has_native_call(&module, "alloc_record"), "StructNew should emit alloc_record native call");
    assert!(module_has_native_call(&module, "record_set"), "StructNew should emit record_set native call for each field");
    assert!(module_invokes_native(&module, "alloc_record"), "bytecode should contain CallNative(alloc_record)");
    assert!(module_invokes_native(&module, "record_set"), "bytecode should contain CallNative(record_set)");
}

#[test]
fn nyar_vm_aggregate_copy_lowers() {
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
    // AggregateCopy 必须分配新 Record 并逐字段复制：record_get 读源、record_set 写目标。
    assert!(module_has_native_call(&module, "alloc_record"), "AggregateCopy should emit alloc_record for new record");
    assert!(module_has_native_call(&module, "record_get"), "AggregateCopy should emit record_get to read source fields");
    assert!(module_has_native_call(&module, "record_set"), "AggregateCopy should emit record_set to write dest fields");
    assert!(module_invokes_native(&module, "record_get"), "bytecode should contain CallNative(record_get)");
}

#[test]
fn nyar_vm_field_get_value_path_uses_record_get() {
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
    // FieldGet Value 路径在 NyarVM 上复用 record_get native call。
    assert!(module_has_native_call(&module, "record_get"), "FieldGet should emit record_get native call");
    assert!(module_invokes_native(&module, "record_get"), "bytecode should contain CallNative(record_get)");
}
