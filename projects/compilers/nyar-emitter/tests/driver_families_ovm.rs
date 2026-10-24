//! `ovm` 家族入口合同测试。
//!
//! `resolve_entry_symbols` 只在 `ovm-lane` 构建中存在（`lib.rs` 的重导出与
//! `driver::families::ovm` 模块同为该 feature 门控），因此本目标只在启用该
//! feature 时可编译；断言与生产代码不变。
#![cfg(feature = "ovm-lane")]

use nyar_emitter::{FragmentSubmission, TargetExecutionPlan, resolve_entry_symbols};
use rusty_valkyrie::{CompilerSourceGroup, ValkyrieCompiler};
use odin_bytecode::{NYAR_VERSION, NyarExport, NyarExportKind, NyarFunction, NyarHeadCode, NyarImport, NyarImportKind, NyarModuleData};

fn module() -> NyarModuleData {
    NyarModuleData {
        version: NYAR_VERSION,
        name: "test".to_owned(),
        constants: Vec::new(),
        functions: vec![NyarFunction { name: "owner::main".to_owned(), arity: 0, local_count: 0, code_offset: 0, code_length: 1 }],
        imports: Vec::new(),
        exports: vec![NyarExport { kind: NyarExportKind::Function, symbol_name: "owner::main".to_owned(), function_index: 0 }],
        witness_entries: Vec::new(),
        code_bytes: vec![0x05],
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: Vec::new(),
    }
}

#[test]
fn entry_contract_rejects_suffix_and_unexported_function() {
    let mut module = module();
    assert!(resolve_entry_symbols(&module, &[]).is_err());
    assert_eq!(resolve_entry_symbols(&module, &["owner::main".to_owned()]).unwrap(), vec!["owner::main"]);
    module.exports.clear();
    assert!(resolve_entry_symbols(&module, &["owner::main".to_owned()]).is_err());
}

#[test]
fn entry_contract_rejects_ambiguous_and_invalid_exports() {
    let mut module = module();
    module.exports.push(module.exports[0].clone());
    assert!(resolve_entry_symbols(&module, &["owner::main".to_owned()]).is_err());
    module.exports.pop();
    module.exports[0].function_index = -1;
    assert!(resolve_entry_symbols(&module, &["owner::main".to_owned()]).is_err());
    module.exports[0].function_index = 1;
    assert!(resolve_entry_symbols(&module, &["owner::main".to_owned()]).is_err());
}

#[test]
fn entry_contract_preserves_all_declared_library_exports() {
    let mut module = module();
    module.exports.push(NyarExport { kind: NyarExportKind::Function, symbol_name: "second".to_owned(), function_index: 0 });
    let entries = vec!["second".to_owned(), "owner::main".to_owned()];
    assert_eq!(resolve_entry_symbols(&module, &entries).unwrap(), entries);
}

/// 沿 Compiler 产出的目标私有计划降低到 `.nyar`，不重放调用边。
fn try_lower_source_groups(groups: &[CompilerSourceGroup]) -> Result<NyarModuleData, String> {
    let program = ValkyrieCompiler::default()
        .compile_source_groups_to_program_with_host_bindings(groups, &[], "native")
        .expect("源码闭包必须完成 Compiler 链路");
    let roots = program.canonical().mir.functions.keys().copied().collect::<Vec<_>>();
    let fragment = program.canonical().linked.fragments.keys().next().expect("测试程序具有片段").clone();
    let plan = TargetExecutionPlan::from_compiled_program(&program, &fragment, nyar::TheoryBundle::default(), &roots)
        .map_err(|error| error.to_string())?;
    nyar_emitter::lowering::backends::ovm::lower_fragment_to_nyar_module(&FragmentSubmission::from_backend_plan(plan))
        .map_err(|error| error.to_string())
}

fn lower_source_groups(groups: &[CompilerSourceGroup]) -> NyarModuleData {
    try_lower_source_groups(groups).expect("只允许沿已绑定实例发射")
}

fn decoded_codes(module: &NyarModuleData) -> Vec<NyarHeadCode> {
    let mut codes = Vec::new();
    let mut position = 0;
    while position < module.code_bytes.len() {
        let instruction = odin_bytecode::decode_at(&module.code_bytes, position);
        assert!(instruction.size > 0, "字节码必须可解码");
        codes.push(instruction.code);
        position += instruction.size as usize;
    }
    codes
}

/// 数值 primitive 声明组 + 入口组；入口组按显式依赖可见性使用 primitive。
fn numeric_groups(primitive: &str, app: &str) -> Vec<CompilerSourceGroup> {
    vec![
        CompilerSourceGroup::single_source("core", "core", primitive),
        CompilerSourceGroup::single_source("app", "demo", app).as_entry().with_direct_dependencies(vec!["core".into()]),
    ]
}

#[test]
fn i32_operator_lowers_to_vm_numeric_opcode_without_host_import() {
    let groups = numeric_groups(
        r#"[primitive("core::primitive::i32")]
structure i32 { }"#,
        r#"
imply i32 {
    infix `+`(self, rhs: Self): Self {
        i32_add(self, rhs)
    }
}

[host_contract]
[ovm_operator("i32.add")]
micro i32_add(a: i32, b: i32): i32

[main]
micro main() -> i32 {
    return 1 + 2
}
"#,
    );
    let module = lower_source_groups(&groups);
    let codes = decoded_codes(&module);
    assert!(codes.contains(&NyarHeadCode::I32Add), "i32 `+` 必须降低为 I32Add 指令，实际 {codes:?}");
    assert!(
        module.imports.iter().all(|import| import.symbol_name != "i32_add" && import.module_name != "nyar.host"),
        "已内联的数值 provider 不得留下 nyar.host 导入: {:?}",
        module.imports
    );
}

/// 未知 `ovm_operator` 助记符不得回落为宿主导入或默认指令，必须返回结构化 backend error。
#[test]
fn unknown_ovm_operator_mnemonic_fails_structurally() {
    let groups = numeric_groups(
        r#"[primitive("core::primitive::i32")]
structure i32 { }"#,
        r#"
imply i32 {
    infix `+`(self, rhs: Self): Self {
        i32_add(self, rhs)
    }
}

[host_contract]
[ovm_operator("i32.unknown")]
micro i32_add(a: i32, b: i32): i32

[main]
micro main() -> i32 {
    return 1 + 2
}
"#,
    );
    let message = try_lower_source_groups(&groups).expect_err("未知 `ovm_operator` 助记符必须结构化失败");
    assert!(message.contains("ovm_operator"), "{message}");
    assert!(message.contains("i32.unknown"), "{message}");
    assert!(message.contains("function="), "错误必须带函数符号: {message}");
}

/// 助记符 arity 与实参数量不符时不得发射指令，必须返回结构化 backend error。
#[test]
fn ovm_operator_arity_mismatch_fails_structurally() {
    let groups = numeric_groups(
        r#"[primitive("core::primitive::i32")]
structure i32 { }"#,
        r#"
[host_contract]
[ovm_operator("i32.neg")]
micro i32_neg(a: i32, b: i32): i32

[main]
micro main() -> i32 {
    return i32_neg(1, 2)
}
"#,
    );
    let message = try_lower_source_groups(&groups).expect_err("助记符 arity 与实参数量不符必须结构化失败");
    assert!(message.contains("i32.neg"), "{message}");
    assert!(message.contains("expected=1"), "错误必须带期望 arity: {message}");
    assert!(message.contains("actual=2"), "错误必须带实际实参数: {message}");
    assert!(message.contains("function="), "错误必须带函数符号: {message}");
}

#[test]
fn i64_add_provider_stays_a_nyar_host_import() {
    let groups = numeric_groups(
        r#"[primitive("core::primitive::i64")]
structure i64 { }"#,
        r#"
[host_contract]
[ovm("i64_add")]
micro i64_add(a: i64, b: i64): i64

[main]
micro main() -> i64 {
    return i64_add(1, 2)
}
"#,
    );
    let module = lower_source_groups(&groups);
    let import = module
        .imports
        .iter()
        .position(|import| import.module_name == "nyar.host" && import.symbol_name == "i64_add")
        .expect("i64_add 必须保留 nyar.host 导入");
    assert_eq!(
        module.imports[import],
        NyarImport { kind: NyarImportKind::Function, module_name: "nyar.host".into(), symbol_name: "i64_add".into() }
    );
    let mut position = 0;
    let mut call_import = None;
    while position < module.code_bytes.len() {
        let instruction = odin_bytecode::decode_at(&module.code_bytes, position);
        if instruction.code == NyarHeadCode::CallImport && instruction.operand1 == import as i32 {
            call_import = Some(instruction);
        }
        position += instruction.size as usize;
    }
    let call_import = call_import.expect("i64_add 调用必须发射 CallImport");
    assert_eq!(call_import.operand2, 2, "CallImport argc 必须等于实参个数");
}

/// `i32.neg` 在 ovm lane 同样按计划冻结的步骤发射：先 `Constant 0`，再 `I32Sub`，
/// backend 不自行判断取负。
#[test]
fn vm_i32_neg_recipe_emits_zero_constant_before_sub() {
    let groups = numeric_groups(
        r#"[primitive("core::primitive::i32")]
structure i32 { }"#,
        r#"
[host_contract]
[ovm_operator("i32.neg")]
micro i32_neg(a: i32): i32

[main]
micro main() -> i32 {
    return i32_neg(7)
}
"#,
    );
    let module = lower_source_groups(&groups);
    let codes = decoded_codes(&module);
    let constant = codes.iter().position(|code| *code == NyarHeadCode::Constant).expect("配方必须先压零常量");
    let sub = codes.iter().position(|code| *code == NyarHeadCode::I32Sub).expect("配方必须发射 I32Sub");
    assert!(constant < sub, "零常量必须先于 I32Sub，实际 {codes:?}");
    assert!(module.imports.iter().all(|import| import.symbol_name != "i32_neg"), "已内联的取负实现不得留下宿主导入: {:?}", module.imports);
}
