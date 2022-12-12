use nyar_emitter::{
    FragmentSubmission,
    nyar_backend_clr::{
        MsilAssembly, MsilInstruction, MsilInstructionOperand, MsilMethodBody, MsilMethodRef, MsilMethodSignature, MsilModule, MsilOpcode,
        MsilType, MsilTypeDef,
    },
    nyar_backend_jvm::JvmInstruction,
    nyar_backend_wasi::{WasmBinaryModule, WasmSection},
    testing::{
        append_singleton_metadata_sections, augment_msil_with_singletons, augment_wasm_with_singleton_accessors, build_jvm_singleton_classes,
        decode_wasm_uleb128, singleton_metadata_line,
    },
};
use nyar_language::{
    AggregateLayout, AggregateLayoutPlan, FieldLayout, MirStorageKind, SingletonInstancePlan,
    mir::singleton::{SINGLETON_CONSTRUCTOR_NAME, SINGLETON_FINALIZER_NAME, SINGLETON_UNLOAD_ACCESSOR},
    types::hir::ValkyrieType,
};

fn counter_submission(lazy: bool) -> FragmentSubmission {
    counter_submission_with_lifecycle(lazy, false, false)
}

fn counter_submission_with_lifecycle(lazy: bool, with_ctor: bool, with_finalizer: bool) -> FragmentSubmission {
    FragmentSubmission {
        aggregate_layouts: AggregateLayoutPlan {
            layouts: vec![AggregateLayout {
                id: 1,
                name: "Counter".to_string(),
                namespace: String::new(),
                storage: MirStorageKind::Reference,
                size: 8,
                align: 8,
                fields: vec![FieldLayout {
                    name: "total".to_string(),
                    ty: nyar::NyarType::Integer64 { signed: true },
                    offset: 0,
                    size: 8,
                    align: 8,
                }],
            }],
            value_type_names: Default::default(),
            type_name_to_layout: Default::default(),
        },
        singleton_instances: vec![SingletonInstancePlan {
            name: "Counter".to_string(),
            namespace: String::new(),
            instance_field: "INSTANCE".to_string(),
            is_lazy: lazy,
            constructor_symbol: if with_ctor { Some("Counter.init".to_string()) } else { None },
            finalizer_symbol: if with_finalizer { Some("Counter.finalize".to_string()) } else { None },
        }],
        ..FragmentSubmission::default()
    }
}

fn minimal_clr_module() -> MsilModule {
    MsilModule {
        assembly: MsilAssembly { name: "test".to_string(), externs: Vec::new() },
        types: vec![MsilTypeDef {
            full_name: "Counter".to_string(),
            namespace: String::new(),
            fields: Vec::new(),
            methods: vec![MsilMethodBody {
                method: MsilMethodRef {
                    owner: Some("Counter".to_string()),
                    name: ".ctor".to_string(),
                    signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
                },
                locals: Vec::new(),
                instructions: vec![MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None }],
                max_stack: 8,
                is_entry_point: false,
                is_async: false,
            }],
            is_value_type: false,
        }],
        global_methods: Vec::new(),
    }
}

fn minimal_wasm_module() -> WasmBinaryModule {
    let mut module = WasmBinaryModule::new();
    module.sections.push(WasmSection { id: 1, name: None, bytes: vec![1, 0x60, 0, 0] });
    module.sections.push(WasmSection { id: 3, name: None, bytes: vec![1, 0] });
    module.sections.push(WasmSection {
        id: 7,
        name: None,
        bytes: {
            let mut bytes = vec![1];
            bytes.push(6);
            bytes.extend_from_slice(b"_start");
            bytes.push(0x00);
            bytes.push(0);
            bytes
        },
    });
    module.sections.push(WasmSection { id: 10, name: None, bytes: vec![1, 2, 0x00, 0x0B] });
    module
}

#[test]
fn eager_singleton_emits_cctor_and_instance_accessor() {
    let submission = counter_submission(false);
    let mut module = minimal_clr_module();

    augment_msil_with_singletons(&submission, &mut module).expect("singleton CLR lowering");
    let counter = &module.types[0];
    assert!(counter.fields.iter().any(|field| field.name == "INSTANCE" && field.is_static));
    assert!(counter.methods.iter().any(|method| method.method.name == ".cctor"));
    assert!(counter.methods.iter().any(|method| method.method.name == "instance"));
    assert!(!counter.methods.iter().any(|method| method.method.name == "get_instance"));
}

#[test]
fn lazy_singleton_emits_get_instance_only() {
    let submission = counter_submission(true);
    let mut module = minimal_clr_module();

    augment_msil_with_singletons(&submission, &mut module).expect("singleton CLR lowering");
    let counter = &module.types[0];
    assert!(counter.methods.iter().any(|method| method.method.name == "get_instance"));
    assert!(!counter.methods.iter().any(|method| method.method.name == ".cctor"));
}

#[test]
fn jvm_eager_singleton_emits_clinit_and_instance() {
    let classes = build_jvm_singleton_classes(&counter_submission(false));
    assert_eq!(classes.len(), 1);
    let counter = &classes[0];
    assert!(counter.fields.iter().any(|field| field.name == "INSTANCE" && field.access_flags & 0x0008 != 0));
    assert!(counter.methods.iter().any(|method| method.name == "<clinit>"));
    assert!(counter.methods.iter().any(|method| method.name == "instance"));
}

#[test]
fn jvm_lazy_singleton_emits_get_instance() {
    let classes = build_jvm_singleton_classes(&counter_submission(true));
    assert_eq!(classes.len(), 1);
    let counter = &classes[0];
    assert!(counter.methods.iter().any(|method| method.name == "get_instance"));
    assert!(!counter.methods.iter().any(|method| method.name == "<clinit>"));
}

#[test]
fn wasm_eager_singleton_emits_global_and_accessor_export() {
    let submission = counter_submission(false);
    let mut module = minimal_wasm_module();
    augment_wasm_with_singleton_accessors(&mut module, &submission);

    assert!(module.sections.iter().any(|section| section.id == 6));
    let export_section = module.sections.iter().find(|section| section.id == 7).expect("export section");
    assert!(String::from_utf8_lossy(&export_section.bytes).contains("Counter__instance"));
    let code_section = module.sections.iter().find(|section| section.id == 10).expect("code section");
    let mut pos = 0;
    let code_count = decode_wasm_uleb128(&code_section.bytes, &mut pos);
    assert_eq!(code_count, 2);
}

#[test]
fn wasm_lazy_singleton_emits_global_and_get_instance_export() {
    let submission = counter_submission(true);
    let mut module = minimal_wasm_module();
    augment_wasm_with_singleton_accessors(&mut module, &submission);

    assert!(module.sections.iter().any(|section| section.id == 6));
    let export_section = module.sections.iter().find(|section| section.id == 7).expect("export section");
    assert!(String::from_utf8_lossy(&export_section.bytes).contains("Counter__get_instance"));
}

#[test]
fn wasm_singleton_metadata_uses_unified_schema() {
    let submission = counter_submission(false);
    let mut module = minimal_wasm_module();
    append_singleton_metadata_sections(&mut module, &submission);

    let singleton_section = module
        .sections
        .iter()
        .find(|section| section.id == 0 && section.name.as_deref() == Some("legion.singleton.Counter"))
        .expect("legion.singleton.Counter custom section");
    let payload = String::from_utf8_lossy(&singleton_section.bytes);
    assert!(payload.contains("|Counter|INSTANCE|static|instance|-|-|-"));
}

#[test]
fn wasm_lazy_singleton_metadata_uses_lazy_mode() {
    let submission = counter_submission(true);
    let mut module = minimal_wasm_module();
    append_singleton_metadata_sections(&mut module, &submission);

    let singleton_section = module
        .sections
        .iter()
        .find(|section| section.id == 0 && section.name.as_deref() == Some("legion.singleton.Counter"))
        .expect("legion.singleton.Counter custom section");
    let payload = String::from_utf8_lossy(&singleton_section.bytes);
    assert!(payload.contains("|Counter|INSTANCE|lazy|get_instance|-|-|unload"));
}

#[test]
fn clr_eager_singleton_with_constructor_emits_init_call_in_cctor() {
    let submission = counter_submission_with_lifecycle(false, true, false);
    let mut module = minimal_clr_module();
    augment_msil_with_singletons(&submission, &mut module).expect("singleton CLR lowering");
    let cctor = module.types[0].methods.iter().find(|method| method.method.name == ".cctor").expect(".cctor should exist for eager singleton");
    let has_init_call = cctor.instructions.iter().any(|instruction| {
        matches!(&instruction.operand, Some(MsilInstructionOperand::Method(reference)) if reference.name == SINGLETON_CONSTRUCTOR_NAME)
    });
    assert!(has_init_call);
}

#[test]
fn clr_lazy_singleton_with_finalizer_emits_unload_method() {
    let submission = counter_submission_with_lifecycle(true, false, true);
    let mut module = minimal_clr_module();
    augment_msil_with_singletons(&submission, &mut module).expect("singleton CLR lowering");
    let counter = &module.types[0];
    let unload = counter
        .methods
        .iter()
        .find(|method| method.method.name == SINGLETON_UNLOAD_ACCESSOR)
        .expect("unload method should exist for lazy singleton");
    let has_finalize_call = unload.instructions.iter().any(|instruction| {
        matches!(&instruction.operand, Some(MsilInstructionOperand::Method(reference)) if reference.name == SINGLETON_FINALIZER_NAME)
    });
    assert!(has_finalize_call);
}

#[test]
fn clr_eager_singleton_without_constructor_omits_init_call() {
    let submission = counter_submission(false);
    let mut module = minimal_clr_module();
    augment_msil_with_singletons(&submission, &mut module).expect("singleton CLR lowering");
    let cctor = module.types[0].methods.iter().find(|method| method.method.name == ".cctor").expect(".cctor should exist");
    let has_init_call = cctor.instructions.iter().any(|instruction| {
        matches!(&instruction.operand, Some(MsilInstructionOperand::Method(reference)) if reference.name == SINGLETON_CONSTRUCTOR_NAME)
    });
    assert!(!has_init_call);
}

#[test]
fn jvm_eager_singleton_with_constructor_emits_init_call_in_clinit() {
    let submission = counter_submission_with_lifecycle(false, true, false);
    let classes = build_jvm_singleton_classes(&submission);
    assert_eq!(classes.len(), 1);
    let clinit = classes[0].methods.iter().find(|method| method.name == "<clinit>").expect("<clinit> should exist for eager singleton");
    let code = clinit.code.as_ref().expect("<clinit> must have code body");
    let has_init_call = code
        .instructions
        .iter()
        .any(|instruction| matches!(instruction, JvmInstruction::InvokeVirtual(reference) if reference.name == SINGLETON_CONSTRUCTOR_NAME));
    assert!(has_init_call);
}

#[test]
fn jvm_lazy_singleton_with_finalizer_emits_unload_method() {
    let submission = counter_submission_with_lifecycle(true, false, true);
    let classes = build_jvm_singleton_classes(&submission);
    assert_eq!(classes.len(), 1);
    let unload = classes[0]
        .methods
        .iter()
        .find(|method| method.name == SINGLETON_UNLOAD_ACCESSOR)
        .expect("unload method should exist for lazy singleton with finalizer");
    let code = unload.code.as_ref().expect("unload must have code body");
    let has_finalize_call = code
        .instructions
        .iter()
        .any(|instruction| matches!(instruction, JvmInstruction::InvokeVirtual(reference) if reference.name == SINGLETON_FINALIZER_NAME));
    assert!(has_finalize_call);
}

#[test]
fn jvm_lazy_singleton_with_constructor_emits_init_in_get_instance() {
    let submission = counter_submission_with_lifecycle(true, true, false);
    let classes = build_jvm_singleton_classes(&submission);
    assert_eq!(classes.len(), 1);
    let get_instance =
        classes[0].methods.iter().find(|method| method.name == "get_instance").expect("get_instance should exist for lazy singleton");
    let code = get_instance.code.as_ref().expect("get_instance must have code body");
    let has_init_call = code
        .instructions
        .iter()
        .any(|instruction| matches!(instruction, JvmInstruction::InvokeVirtual(reference) if reference.name == SINGLETON_CONSTRUCTOR_NAME));
    assert!(has_init_call);
}

#[test]
fn metadata_line_includes_constructor_and_finalizer_symbols() {
    let submission = counter_submission_with_lifecycle(true, true, true);
    let plan = &submission.singleton_instances[0];
    let line = singleton_metadata_line(plan);
    assert!(line.contains("|Counter|INSTANCE|lazy|get_instance|Counter.init|Counter.finalize|unload"));
}

#[test]
fn metadata_line_uses_dash_for_absent_lifecycle() {
    let submission = counter_submission(false);
    let plan = &submission.singleton_instances[0];
    let line = singleton_metadata_line(plan);
    assert!(line.contains("|Counter|INSTANCE|static|instance|-|-|-"));
}
