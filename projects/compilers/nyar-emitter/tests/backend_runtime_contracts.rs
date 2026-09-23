use std_data::binary::{
    elf::{NativeElfImage, NativeElfWriter, SharedElfImage, SharedElfWriter, SharedObjectExport},
    nyar_ir::{NyarConstant, NyarFunction, NyarModuleData},
    pe::{NativePeImage, NativePeWriter},
};

use nyar_emitter::{
    nyar_backend_native::{NativeExecutableKind, classify_native_executable},
    nyar_backend_vm::emit_nyar_module,
    nyar_backend_wasi::{WasmTraitFatPointer, plan_witness_table_layout, resolve_witness_call},
};

#[test]
fn emits_minimal_module_bytes() {
    let module = NyarModuleData {
        version: 1,
        name: "demo".to_string(),
        constants: vec![NyarConstant::Integer32(0)],
        functions: vec![NyarFunction { name: "main".to_string(), arity: 0, local_count: 0, code_offset: 0, code_length: 1 }],
        imports: Vec::new(),
        exports: Vec::new(),
        witness_entries: Vec::new(),
        code_bytes: vec![0x30],
        globals: Vec::new(),
        init_function_indices: Vec::new(),
    };
    let path = std::env::temp_dir().join(format!("nyar-backend-vm-{}.nyar", std::process::id()));
    emit_nyar_module(&module, &path).expect("emit");
    assert!(path.is_file());
    let _ = std::fs::remove_file(path);
}

#[test]
fn classifies_writer_outputs() {
    let pe = NativePeWriter::write_executable(&NativePeImage {
        text: vec![0xC3],
        rdata: Vec::new(),
        idata: Vec::new(),
        imports: Vec::new(),
        entry_point: 0,
    })
    .expect("pe");
    assert_eq!(classify_native_executable(&pe).expect("pe"), NativeExecutableKind::Pe);

    let elf = NativeElfWriter::write_executable(&NativeElfImage { text: vec![0xC3], rodata: Vec::new(), entry_point: 0 }).expect("elf");
    assert_eq!(classify_native_executable(&elf).expect("elf"), NativeExecutableKind::Elf);

    let shared = SharedElfWriter::write_aarch64(
        &SharedElfImage { text: vec![0xC0, 0x03, 0x5F, 0xD6], rodata: Vec::new(), bss: Vec::new() },
        &[SharedObjectExport { name: "asgard_invoke_export".into(), text_offset: 0 }],
    )
    .expect("so");
    assert_eq!(classify_native_executable(&shared).expect("so"), NativeExecutableKind::ElfShared);
}

#[test]
fn witness_table_uses_non_intrusive_fat_pointer() {
    let layout = plan_witness_table_layout("Dog", "Animal", &["make_sound", "name"]);
    assert_eq!(layout.type_name, "Dog");
    assert_eq!(layout.trait_name, "Animal");
    assert_eq!(layout.methods.len(), 2);
    assert_eq!(resolve_witness_call(&layout, 1), Some(1));
}

#[test]
fn fat_pointer_carries_data_and_witness_separately() {
    let fat = WasmTraitFatPointer { data_ptr: 0x100, witness_ptr: 0x200 };
    assert_ne!(fat.data_ptr, fat.witness_ptr);
}
