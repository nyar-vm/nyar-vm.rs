//! NyarVM value-storage lowering coverage that needs the language frontend.
//!
//! Lives here (not in `emitter` unit tests) so the driver crate stays free of
//! `nyar-language` while still covering compiler → executable → VM bytecode.
//!
//! Requires `emitter/legacy-lanes`（nyar_vm bytecode 车道；见本仓 Cargo feature `nyar-vm-lane`）。

use std::sync::Arc;

use emitter::{FragmentSubmission, executable_provider::MirFunctionMapProvider, testing};
use nyar::Identifier;
use nyar_language::{MirLowerer, ValkyrieCompiler, mir_function_to_executable, types::SourceID};
use std_data::binary::nyar_ir::{NyarHeadCode, NyarModuleData};

/// 模块 imports 表是否声明指定宿主符号（`nyar.host`）。
fn module_declares_host_import(module: &NyarModuleData, name: &str) -> bool {
    module.imports.iter().any(|import| import.module_name == "nyar.host" && import.symbol_name == name)
}

/// 字节码是否含 `CallImport`，且 operand1 为该宿主符号在 imports 表中的下标。
fn module_invokes_import(module: &NyarModuleData, name: &str) -> bool {
    let Some(import_index) =
        module.imports.iter().position(|import| import.module_name == "nyar.host" && import.symbol_name == name)
    else {
        return false;
    };
    let index_bytes = (import_index as i32).to_le_bytes();
    module.code_bytes.windows(9).any(|window| window[0] == NyarHeadCode::CallImport as u8 && window[1..5] == index_bytes)
}

fn lower_main_from_source(source: &str, version_id: u32) -> NyarModuleData {
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
    // StructNew Value 路径经 CallImport(alloc_record) 构造 Record，再 CallImport(record_set) 写字段。
    assert!(module_declares_host_import(&module, "alloc_record"), "StructNew should declare alloc_record import");
    assert!(module_declares_host_import(&module, "record_set"), "StructNew should declare record_set import");
    assert!(module_invokes_import(&module, "alloc_record"), "bytecode should contain CallImport(alloc_record)");
    assert!(module_invokes_import(&module, "record_set"), "bytecode should contain CallImport(record_set)");
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
    assert!(module_declares_host_import(&module, "alloc_record"), "AggregateCopy should declare alloc_record");
    assert!(module_declares_host_import(&module, "record_get"), "AggregateCopy should declare record_get");
    assert!(module_declares_host_import(&module, "record_set"), "AggregateCopy should declare record_set");
    assert!(module_invokes_import(&module, "record_get"), "bytecode should contain CallImport(record_get)");
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
    assert!(module_declares_host_import(&module, "record_get"), "FieldGet should declare record_get import");
    assert!(module_invokes_import(&module, "record_get"), "bytecode should contain CallImport(record_get)");
}
