//! `legion spy wasm` 子命令的集成测试。
//!
//! 这些测试在内存中构造最小 `WASM` 二进制，验证 `spy wasm` 的解析、
//! 列表、反汇编与偏移定位能力，不依赖外部文件。
#![cfg(feature = "format-spy")]

use acorn_wasm::{DecodedOperand, decode_code_body};
use legion::{SpyMode, SpyOptions, SpyTargetOptions, run_spy};
use nyar_emitter::nyar_backend_wasi::{WasmBinaryModule, WasmSection};

/// 构造一个最小的 `WASM` 模块：包含一个导出函数 `main`，返回 `i32` 常量 `42`。
fn build_minimal_wasm() -> Vec<u8> {
    let mut module = WasmBinaryModule::new();

    // Type 段（id=1）：1 个函数类型 `() -> i32`
    module.sections.push(WasmSection { id: 1, name: None, bytes: vec![0x01, 0x60, 0x00, 0x01, 0x7F] });

    // Function 段（id=3）：1 个函数，类型索引 0
    module.sections.push(WasmSection { id: 3, name: None, bytes: vec![0x01, 0x00] });

    // Export 段（id=7）：导出函数 `main`（索引 0）
    module.sections.push(WasmSection {
        id: 7,
        name: None,
        bytes: vec![
            0x01, // 1 个导出
            0x04, b'm', b'a', b'i', b'n', // 名称 "main"
            0x00, // kind: func
            0x00, // 函数索引 0
        ],
    });

    // Code 段（id=10）：1 个函数体
    // 函数体：0 个局部变量组，i32.const 42，end
    let func_body = vec![0x00, 0x41, 0x2A, 0x0B];
    let body_size = func_body.len() as u8;
    module.sections.push(WasmSection {
        id: 10,
        name: None,
        bytes: vec![0x01, body_size] // 1 个函数，body 大小
            .into_iter()
            .chain(func_body.iter().copied())
            .collect(),
    });

    module.to_bytes().unwrap()
}

/// 构造一个带 `import` 的 `WASM` 模块：导入 `env.console_log`，导出 `_start`。
fn build_wasm_with_imports() -> Vec<u8> {
    let mut module = WasmBinaryModule::new();

    // Type 段（id=1）：2 个函数类型
    // type 0: (i32) -> ()  — console_log
    // type 1: () -> ()     — _start
    module.sections.push(WasmSection {
        id: 1,
        name: None,
        bytes: vec![
            0x02, // 2 个类型
            0x60, 0x01, 0x7F, 0x00, // type 0: (i32) -> ()
            0x60, 0x00, 0x00, // type 1: () -> ()
        ],
    });

    // Import 段（id=2）：1 个导入函数 `env.console_log`，类型 0
    module.sections.push(WasmSection {
        id: 2,
        name: None,
        bytes: vec![
            0x01, // 1 个导入
            0x03, b'e', b'n', b'v', // 模块名 "env"
            0x0B, b'c', b'o', b'n', b's', b'o', b'l', b'e', b'_', b'l', b'o', b'g', // 字段名 "console_log"
            0x00, // kind: func
            0x00, // 类型索引 0
        ],
    });

    // Function 段（id=3）：1 个函数，类型 1
    module.sections.push(WasmSection { id: 3, name: None, bytes: vec![0x01, 0x01] });

    // Export 段（id=7）：导出 `_start`（函数索引 1，因为索引 0 是导入）
    module.sections.push(WasmSection {
        id: 7,
        name: None,
        bytes: vec![
            0x01, // 1 个导出
            0x06, b'_', b's', b't', b'a', b'r', b't', // 名称 "_start"
            0x00, // kind: func
            0x01, // 函数索引 1
        ],
    });

    // Code 段（id=10）：1 个函数体
    let func_body = vec![
        0x00, // 0 个局部变量组
        0x41, 0x00, // i32.const 0
        0x10, 0x00, // call 0 (console_log)
        0x0B, // end
    ];
    let body_size = func_body.len() as u8;
    module.sections.push(WasmSection {
        id: 10,
        name: None,
        bytes: vec![0x01, body_size].into_iter().chain(func_body.iter().copied()).collect(),
    });

    module.to_bytes().unwrap()
}

#[test]
fn parses_minimal_wasm_magic_and_version() {
    let bytes = build_minimal_wasm();
    let module = WasmBinaryModule::from_bytes(&bytes).unwrap();
    assert_eq!(module.version, 1);
    assert!(!module.sections.is_empty());
}

#[test]
fn minimal_wasm_has_expected_sections() {
    let bytes = build_minimal_wasm();
    let module = WasmBinaryModule::from_bytes(&bytes).unwrap();

    let section_ids: Vec<u8> = module.sections.iter().map(|s| s.id).collect();
    assert!(section_ids.contains(&1), "应包含 Type 段");
    assert!(section_ids.contains(&3), "应包含 Function 段");
    assert!(section_ids.contains(&7), "应包含 Export 段");
    assert!(section_ids.contains(&10), "应包含 Code 段");
}

#[test]
fn wasm_with_imports_parses_correctly() {
    let bytes = build_wasm_with_imports();
    let module = WasmBinaryModule::from_bytes(&bytes).unwrap();

    // 应包含 Import 段
    let import_section = module.sections.iter().find(|s| s.id == 2);
    assert!(import_section.is_some(), "应包含 Import 段");

    // 手动解析 import 段验证内容
    let import_bytes = &import_section.unwrap().bytes;
    assert_eq!(import_bytes[0], 0x01, "应有 1 个导入");

    // 验证模块名 "env"
    let module_name_len = import_bytes[1] as usize;
    assert_eq!(module_name_len, 3);
    assert_eq!(&import_bytes[2..5], b"env");
}

#[test]
fn minimal_wasm_export_name_is_main() {
    let bytes = build_minimal_wasm();
    let module = WasmBinaryModule::from_bytes(&bytes).unwrap();

    let export_section = module.sections.iter().find(|s| s.id == 7).unwrap();
    let export_bytes = &export_section.bytes;

    assert_eq!(export_bytes[0], 0x01, "应有 1 个导出");
    let name_len = export_bytes[1] as usize;
    assert_eq!(name_len, 4);
    assert_eq!(&export_bytes[2..6], b"main");
}

/// 生成 CLI 端到端验证用的 fixture 文件。
///
/// 将最小 `WASM` 模块写入 `target/debug/test_spy.wasm`，
/// 供 `legion spy wasm` 命令行手动验证使用。
#[test]
fn write_fixture_wasm_for_cli() {
    let bytes = build_minimal_wasm();
    let target_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("..").join("..").join("target").join("debug");
    let fixture_path = target_dir.join("test_spy.wasm");

    std::fs::create_dir_all(&target_dir).expect("无法创建 target/debug 目录");
    std::fs::write(&fixture_path, &bytes).expect("无法写入 test_spy.wasm");

    println!("已写入 fixture：{}", fixture_path.display());
}

// ========== W12: CLI 行为测试（含 0xFB GC 指令）==========

/// 构造一个含 `0xFB` GC 指令的 `WASM` 模块。
///
/// 函数体包含 `struct.new_default`（`0xFB 0x01`）与 `array.new_fixed`（`0xFB 0x08`），
/// 用于验证 `spy wasm` 与 `acorn-wasm` 的 `decode_code_body` 正确解码 GC 前缀指令。
fn build_wasm_with_gc_instructions() -> Vec<u8> {
    let mut module = WasmBinaryModule::new();

    // Type 段（id=1）：1 个函数类型 `() -> ()`
    module.sections.push(WasmSection { id: 1, name: None, bytes: vec![0x01, 0x60, 0x00, 0x00] });

    // Function 段（id=3）：1 个函数，类型索引 0
    module.sections.push(WasmSection { id: 3, name: None, bytes: vec![0x01, 0x00] });

    // Export 段（id=7）：导出函数 `gc_fn`（索引 0）
    module.sections.push(WasmSection { id: 7, name: None, bytes: vec![0x01, 0x05, b'g', b'c', b'_', b'f', b'n', 0x00, 0x00] });

    // Code 段（id=10）：1 个函数体，含 0xFB GC 指令
    let func_body = vec![
        0x00, // 0 个局部变量组
        0xFB, 0x01, 0x00, // struct.new_default type_idx=0
        0xFB, 0x08, 0x00, 0x01, // array.new_fixed type_idx=0 count=1
        0x41, 0x2A, // i32.const 42
        0x0B, // end
    ];
    let body_size = func_body.len() as u8;
    module.sections.push(WasmSection {
        id: 10,
        name: None,
        bytes: vec![0x01, body_size].into_iter().chain(func_body.iter().copied()).collect(),
    });

    module.to_bytes().unwrap()
}

/// 构造 `spy wasm` 的 `SpyOptions`，便于各 CLI 行为测试按需覆盖字段。
fn build_wasm_spy_opts(input: &str, func: Option<&str>, offset: Option<i64>, json: bool, hex: bool) -> SpyOptions {
    SpyOptions {
        mode: SpyMode::Wasm(SpyTargetOptions {
            input: Some(input.to_string()),
            func: func.map(String::from),
            method: None,
            offset,
            list: false,
            context: 20,
            target_platform: None,
            json,
            hex,
            types: false,
            gc_audit: false,
            glue_audit: false,
        }),
    }
}

/// 将 `WASM` 字节写入临时文件，返回临时目录与文件路径字符串。
fn write_gc_wasm_fixture() -> (tempfile::TempDir, String) {
    let temp_dir = tempfile::tempdir().expect("无法创建临时目录");
    let bytes = build_wasm_with_gc_instructions();
    let wasm_path = temp_dir.path().join("gc_test.wasm");
    std::fs::write(&wasm_path, &bytes).expect("无法写入 gc_test.wasm");
    let path_str = wasm_path.to_string_lossy().to_string();
    (temp_dir, path_str)
}

#[test]
fn gc_function_body_decodes_fb_instructions() {
    // 直接构造含 0xFB GC 指令的函数体字节，验证 decode_code_body 解码路径
    let body = vec![
        0x00, // 0 个局部变量组
        0xFB, 0x01, 0x00, // struct.new_default type_idx=0
        0xFB, 0x08, 0x00, 0x01, // array.new_fixed type_idx=0 count=1
        0x41, 0x2A, // i32.const 42
        0x0B, // end
    ];
    let instructions = decode_code_body(&body);

    // 应解码出 4 条指令（struct.new_default / array.new_fixed / i32.const / end）
    assert_eq!(instructions.len(), 4, "应解码出 4 条指令");

    // struct.new_default type_idx=0
    assert_eq!(instructions[0].mnemonic, "struct.new_default");
    assert_eq!(instructions[0].opcode, 0xFB);
    assert_eq!(instructions[0].operands, vec![DecodedOperand::TypeIndex(0)]);

    // array.new_fixed type_idx=0 count=1
    assert_eq!(instructions[1].mnemonic, "array.new_fixed");
    assert_eq!(instructions[1].opcode, 0xFB);
    assert_eq!(instructions[1].operands, vec![DecodedOperand::TypeIndex(0), DecodedOperand::Count(1)]);

    // i32.const 42
    assert_eq!(instructions[2].mnemonic, "i32.const");
    assert_eq!(instructions[2].operands, vec![DecodedOperand::ValueI32(42)]);

    // end
    assert_eq!(instructions[3].mnemonic, "end");
}

#[test]
fn spy_wasm_overview_mode_succeeds() {
    let (_temp_dir, path) = write_gc_wasm_fixture();
    let opts = build_wasm_spy_opts(&path, None, None, false, false);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "overview/列表模式应成功：{:?}", result.err());
}

#[test]
fn spy_wasm_func_mode_disassembles_gc() {
    let (_temp_dir, path) = write_gc_wasm_fixture();
    let opts = build_wasm_spy_opts(&path, Some("0"), None, false, false);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "--func 反汇编模式应成功：{:?}", result.err());
}

#[test]
fn spy_wasm_json_mode_succeeds() {
    let (_temp_dir, path) = write_gc_wasm_fixture();
    let opts = build_wasm_spy_opts(&path, Some("0"), None, true, false);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "--func --json 模式应成功：{:?}", result.err());
}

#[test]
fn spy_wasm_hex_mode_succeeds() {
    let (_temp_dir, path) = write_gc_wasm_fixture();
    let opts = build_wasm_spy_opts(&path, Some("0"), None, false, true);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "--func --hex 模式应成功：{:?}", result.err());
}

#[test]
fn spy_wasm_offset_mode_locates_gc_instruction() {
    let bytes = build_wasm_with_gc_instructions();
    let temp_dir = tempfile::tempdir().expect("无法创建临时目录");
    let wasm_path = temp_dir.path().join("gc_offset_test.wasm");
    std::fs::write(&wasm_path, &bytes).expect("无法写入 gc_offset_test.wasm");
    let path_str = wasm_path.to_string_lossy().to_string();

    // 定位 struct.new_default（0xFB 0x01）在文件中的绝对偏移
    let gc_offset = bytes.windows(2).position(|w| w == &[0xFB, 0x01]).expect("应包含 struct.new_default 指令字节");

    let opts = build_wasm_spy_opts(&path_str, None, Some(gc_offset as i64), false, false);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "--offset 定位模式应成功：{:?}", result.err());
}

// ========== S14: section 展示回归测试 ==========

/// `--types` 模式输出 type 段条目（func/struct/array）。
///
/// 使用含 1 个函数类型 `() -> ()` 的 GC wasm fixture，验证 `--types` 模式成功执行。
#[test]
fn spy_wasm_types_mode_dumps_type_section() {
    let (_temp_dir, path) = write_gc_wasm_fixture();
    let opts = SpyOptions {
        mode: SpyMode::Wasm(SpyTargetOptions {
            input: Some(path),
            func: None,
            method: None,
            offset: None,
            list: false,
            context: 20,
            target_platform: None,
            json: false,
            hex: false,
            types: true,
            gc_audit: false,
            glue_audit: false,
        }),
    };
    let result = run_spy(&opts);
    assert!(result.is_ok(), "--types 模式应成功：{:?}", result.err());
}

/// `--types --json` 模式输出有效 type 段 JSON。
#[test]
fn spy_wasm_types_mode_json_output() {
    let (_temp_dir, path) = write_gc_wasm_fixture();
    let opts = SpyOptions {
        mode: SpyMode::Wasm(SpyTargetOptions {
            input: Some(path),
            func: None,
            method: None,
            offset: None,
            list: false,
            context: 20,
            target_platform: None,
            json: true,
            hex: false,
            types: true,
            gc_audit: false,
            glue_audit: false,
        }),
    };
    let result = run_spy(&opts);
    assert!(result.is_ok(), "--types --json 模式应成功：{:?}", result.err());
}

/// `--func 1` 验证函数索引计算与 import 偏移。
///
/// `build_wasm_with_imports` 有 1 个导入函数（索引 0）和 1 个本地函数（索引 1），
/// `--func 1` 应正确定位并反汇编本地函数 `_start`。
#[test]
fn spy_wasm_func_with_import_offset_disassembles() {
    let bytes = build_wasm_with_imports();
    let temp_dir = tempfile::tempdir().expect("无法创建临时目录");
    let wasm_path = temp_dir.path().join("import_offset_test.wasm");
    std::fs::write(&wasm_path, &bytes).expect("无法写入 import_offset_test.wasm");
    let path_str = wasm_path.to_string_lossy().to_string();

    let opts = build_wasm_spy_opts(&path_str, Some("1"), None, false, false);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "--func 1（含导入偏移）应成功：{:?}", result.err());
}

/// `--list` 模式（默认 overview）在含 Imports/Exports 的模块上无回归。
#[test]
fn spy_wasm_list_mode_with_imports_and_exports() {
    let bytes = build_wasm_with_imports();
    let temp_dir = tempfile::tempdir().expect("无法创建临时目录");
    let wasm_path = temp_dir.path().join("list_imports_test.wasm");
    std::fs::write(&wasm_path, &bytes).expect("无法写入 list_imports_test.wasm");
    let path_str = wasm_path.to_string_lossy().to_string();

    let opts = build_wasm_spy_opts(&path_str, None, None, false, false);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "--list 模式（含 imports/exports）应成功：{:?}", result.err());
}

/// `--json` 模式在含 Imports/Exports 的模块上输出有效 JSON。
#[test]
fn spy_wasm_json_mode_with_imports_succeeds() {
    let bytes = build_wasm_with_imports();
    let temp_dir = tempfile::tempdir().expect("无法创建临时目录");
    let wasm_path = temp_dir.path().join("json_imports_test.wasm");
    std::fs::write(&wasm_path, &bytes).expect("无法写入 json_imports_test.wasm");
    let path_str = wasm_path.to_string_lossy().to_string();

    let opts = build_wasm_spy_opts(&path_str, None, None, true, false);
    let result = run_spy(&opts);
    assert!(result.is_ok(), "--json 模式（含 imports/exports）应成功：{:?}", result.err());
}
