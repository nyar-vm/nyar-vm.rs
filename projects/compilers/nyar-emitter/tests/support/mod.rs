use std::path::Path;

use nyar::{
    BackendInputKind, BinaryTarget, HostProjectionBoundary, PartitionBackendRequirement, ReferenceManagement, TargetLane,
    backends::CompilationOptions,
};
use nyar_emitter::{
    LoweredBackendInput,
    nyar_backend_jvm::{JvmClassFile, JvmCodeBody, JvmInstruction, JvmMethodDescriptor, JvmMethodSignature, JvmTypeDescriptor},
    nyar_backend_wasi::WasmBinaryModule,
};

fn encode_uleb128(mut value: u32, out: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn encode_name(name: &str, out: &mut Vec<u8>) {
    encode_uleb128(name.len() as u32, out);
    out.extend_from_slice(name.as_bytes());
}

fn wasm_section(id: u8, payload: Vec<u8>) -> Vec<u8> {
    let mut section = vec![id];
    encode_uleb128(payload.len() as u32, &mut section);
    section.extend_from_slice(&payload);
    section
}

fn minimal_wasi_command_module_with_imports(imports: &[(String, String)]) -> WasmBinaryModule {
    minimal_wasi_command_module_with_imports_for(imports, nyar_emitter::nyar_backend_wasi::WasiPreview::Preview2)
}

fn minimal_wasi_command_module_with_imports_for(
    imports: &[(String, String)],
    preview: nyar_emitter::nyar_backend_wasi::WasiPreview,
) -> WasmBinaryModule {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]);

    // types: command helpers plus the explicit canonical import signatures.
    let mut type_payload = Vec::new();
    encode_uleb128(5, &mut type_payload);
    type_payload.extend_from_slice(&[0x60, 0x00, 0x00]);
    type_payload.extend_from_slice(&[0x60, 0x04, 0x7F, 0x7F, 0x7F, 0x7F, 0x01, 0x7F]);
    type_payload.extend_from_slice(&[0x60, 0x00, 0x01, 0x7F]);
    type_payload.extend_from_slice(&[0x60, 0x00, 0x01, 0x7E]);
    type_payload.extend_from_slice(&[0x60, 0x01, 0x7F, 0x01, 0x7F]);
    bytes.extend_from_slice(&wasm_section(1, type_payload));

    if !imports.is_empty() {
        let mut import_payload = Vec::new();
        encode_uleb128(imports.len() as u32, &mut import_payload);
        for (module, field) in imports {
            encode_name(module, &mut import_payload);
            encode_name(field, &mut import_payload);
            import_payload.push(0x00);
            let type_index = if module == "cm32p2|wasi:clocks/monotonic-clock@0.2" {
                3
            }
            else if module == "cm32p2|wasi:io/streams@0.2" {
                4
            }
            else {
                0
            };
            encode_uleb128(type_index, &mut import_payload);
        }
        bytes.extend_from_slice(&wasm_section(2, import_payload));
    }

    // funcs: _start, run, cabi_post_run, cabi_realloc, _initialize, cli_run_result
    let mut function_payload = Vec::new();
    encode_uleb128(6, &mut function_payload);
    function_payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x01, 0x00, 0x02]);
    bytes.extend_from_slice(&wasm_section(3, function_payload));

    bytes.extend_from_slice(&wasm_section(5, vec![0x01, 0x00, 0x01]));

    let base = imports.len() as u32;
    let cli_run_export = format!("wasi:cli/run@{}#run", preview.package_version());
    let exports: [(&str, u8, u32); 7] = [
        ("_start", 0x00, base),
        ("run", 0x00, base + 1),
        ("cabi_post_run", 0x00, base + 2),
        ("memory", 0x02, 0u32),
        ("cabi_realloc", 0x00, base + 3),
        ("_initialize", 0x00, base + 4),
        (cli_run_export.as_str(), 0x00, base + 5),
    ];
    let mut export_payload = Vec::new();
    encode_uleb128(exports.len() as u32, &mut export_payload);
    for (name, kind, index) in exports {
        encode_name(name, &mut export_payload);
        export_payload.push(kind);
        encode_uleb128(index, &mut export_payload);
    }
    bytes.extend_from_slice(&wasm_section(7, export_payload));

    let code_bodies = [
        vec![0x00, 0x0B],
        vec![0x00, 0x0B],
        vec![0x00, 0x0B],
        vec![0x00, 0x41, 0x00, 0x0B],
        vec![0x00, 0x0B],
        vec![0x00, 0x41, 0x00, 0x0B], // i32.const 0; end
    ];
    let mut code_payload = Vec::new();
    encode_uleb128(code_bodies.len() as u32, &mut code_payload);
    for body in code_bodies {
        encode_uleb128(body.len() as u32, &mut code_payload);
        code_payload.extend_from_slice(&body);
    }
    bytes.extend_from_slice(&wasm_section(10, code_payload));

    WasmBinaryModule::from_bytes(&bytes).expect("valid minimal wasi command module")
}

fn minimal_node_module_with_imports(imports: &[(String, String)]) -> WasmBinaryModule {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]);

    // type 0 = () -> i32 (main and the deliberately minimal fixture imports).
    bytes.extend_from_slice(&wasm_section(1, vec![0x01, 0x60, 0x00, 0x01, 0x7F]));
    if !imports.is_empty() {
        let mut import_payload = Vec::new();
        encode_uleb128(imports.len() as u32, &mut import_payload);
        for (module, field) in imports {
            encode_name(module, &mut import_payload);
            encode_name(field, &mut import_payload);
            import_payload.push(0x00);
            encode_uleb128(0, &mut import_payload);
        }
        bytes.extend_from_slice(&wasm_section(2, import_payload));
    }
    bytes.extend_from_slice(&wasm_section(3, vec![0x01, 0x00]));
    let mut export_payload = Vec::new();
    encode_uleb128(1, &mut export_payload);
    encode_name("main", &mut export_payload);
    export_payload.push(0x00);
    encode_uleb128(imports.len() as u32, &mut export_payload);
    bytes.extend_from_slice(&wasm_section(7, export_payload));
    bytes.extend_from_slice(&wasm_section(10, vec![0x01, 0x04, 0x00, 0x41, 0x00, 0x0B]));
    WasmBinaryModule::from_bytes(&bytes).expect("valid minimal node module")
}

pub fn compilation_options(target: BinaryTarget, artifact_name: &str) -> CompilationOptions {
    CompilationOptions { target, artifact_name: artifact_name.to_string(), emit_debug_symbols: false, optimize: false }
}

pub fn jvm_requirement(target: BinaryTarget) -> PartitionBackendRequirement {
    PartitionBackendRequirement {
        backend_name: "jvm-binary".to_string(),
        interpreter: nyar::Identifier::new("jvm.classfile"),
        fragment: nyar::Identifier::new("functions"),
        lane: TargetLane::Jvm,
        input_kind: BackendInputKind::JvmClassFile,
        target,
        host_boundary: HostProjectionBoundary::Jvm,
        reference_management: ReferenceManagement::HostGc,
    }
}

pub fn wasm_requirement(target: BinaryTarget, host_boundary: HostProjectionBoundary) -> PartitionBackendRequirement {
    PartitionBackendRequirement {
        backend_name: "wasm-binary".to_string(),
        interpreter: nyar::Identifier::new("wasm.module"),
        fragment: nyar::Identifier::new("functions"),
        lane: TargetLane::Wasm,
        input_kind: BackendInputKind::WasmModule,
        target,
        host_boundary,
        reference_management: ReferenceManagement::HostGc,
    }
}

pub fn demo_jvm_input(output_dir: &Path) -> LoweredBackendInput {
    let mut class_file = JvmClassFile::new("demo/Main");
    class_file.methods.push(JvmMethodSignature {
        name: "main".to_string(),
        descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Int),
        access_flags: 0x0001 | 0x0008,
        code: Some(JvmCodeBody { max_stack: 1, max_locals: 0, instructions: vec![JvmInstruction::IConst(1), JvmInstruction::IReturn] }),
    });
    LoweredBackendInput::jvm(nyar_emitter::nyar_backend_jvm::JvmBinaryBackendInput {
        class_file,
        output_dir: output_dir.to_path_buf(),
        emit_class_file: true,
        control_flow: None,
        companion_classes: Vec::new(),
    })
}

pub fn demo_wasm_input(output_dir: &Path, host_boundary: HostProjectionBoundary) -> LoweredBackendInput {
    demo_wasm_input_with_preview(output_dir, host_boundary, nyar_emitter::nyar_backend_wasi::WasiPreview::Preview2)
}

pub fn demo_wasm_input_with_preview(
    output_dir: &Path,
    host_boundary: HostProjectionBoundary,
    wasi_preview: nyar_emitter::nyar_backend_wasi::WasiPreview,
) -> LoweredBackendInput {
    let mut module = match host_boundary {
        HostProjectionBoundary::WasiComponent => minimal_wasi_command_module_with_imports_for(&[], wasi_preview),
        HostProjectionBoundary::WasmJsGlue => minimal_node_module_with_imports(&[]),
        _ => unreachable!("unsupported fixture host boundary"),
    };
    module.push_custom_section("demo.module", b"demo".to_vec());
    LoweredBackendInput::wasm(nyar_emitter::nyar_backend_wasi::WasmBinaryBackendInput {
        module,
        output_dir: output_dir.to_path_buf(),
        host_boundary,
        imports: Vec::new(),
        control_flow: None,
        package_as_wasi_command: matches!(host_boundary, HostProjectionBoundary::WasiComponent),
        wasi_preview,
    })
}

/// 构造带指定 imports 的 `WASM` 后端输入，用于测试契约驱动的 import 绑定。
pub fn demo_wasm_input_with_imports(
    output_dir: &Path,
    host_boundary: HostProjectionBoundary,
    imports: Vec<(String, String)>,
) -> LoweredBackendInput {
    let mut module = match host_boundary {
        HostProjectionBoundary::WasiComponent => {
            let core_imports = imports
                .iter()
                .map(|(module, field)| {
                    let core_module = match module.as_str() {
                        "wasi:clocks/monotonic-clock" => "cm32p2|wasi:clocks/monotonic-clock@0.2",
                        "wasi:io/streams" => "cm32p2|wasi:io/streams@0.2",
                        _ => module.as_str(),
                    };
                    (core_module.to_string(), field.clone())
                })
                .collect::<Vec<_>>();
            minimal_wasi_command_module_with_imports(&core_imports)
        }
        HostProjectionBoundary::WasmJsGlue => minimal_node_module_with_imports(&imports),
        _ => unreachable!("unsupported fixture host boundary"),
    };
    module.push_custom_section("demo.module", b"demo".to_vec());
    LoweredBackendInput::wasm(nyar_emitter::nyar_backend_wasi::WasmBinaryBackendInput {
        module,
        output_dir: output_dir.to_path_buf(),
        host_boundary,
        imports,
        control_flow: None,
        package_as_wasi_command: matches!(host_boundary, HostProjectionBoundary::WasiComponent),
        wasi_preview: nyar_emitter::nyar_backend_wasi::WasiPreview::Preview2,
    })
}
