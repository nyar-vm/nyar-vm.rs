use nyar_emitter::*;
use nyar_language::{CompilerSourceGroup, ValkyrieCompiler};

/// 验证 `wasm_physical_artifact_name` 对任意项目名原样返回，无特殊分支。
#[test]
fn physical_name_preserves_artifact_names() {
    assert_eq!(wasm_physical_artifact_name("demo"), "demo");
    assert_eq!(wasm_physical_artifact_name("legion"), "legion");
    assert_eq!(wasm_physical_artifact_name("legion.tools"), "legion.tools");
    assert_eq!(wasm_physical_artifact_name("main_legion"), "main_legion");
    assert_eq!(wasm_physical_artifact_name("wasm_interop_stdout"), "wasm_interop_stdout");
    assert_eq!(wasm_physical_artifact_name("alpha_entry__main_alpha_entry"), "alpha_entry__main_alpha_entry");
}

/// 与生产 core 同构：数值 primitive 声明 + `infix +` 运算符 + 目标绑定实现点。
fn app_source(mnemonic: &str) -> String {
    format!(
        r#"
[primitive("core::primitive::i32")]
structure i32 {{ }}

imply i32 {{
    infix `+`(self, rhs: Self): Self {{
        return i32_add(self, rhs)
    }}
}}

[wasm_opcode("{mnemonic}")]
abstract micro i32_add(a: i32, b: i32): i32

micro add(a: i32, b: i32) -> i32 {{
    return a + b
}}

[main]
micro main() -> i32 {{
    return add(2, 3)
}}
"#
    )
}

/// 源码 i32 `+` 经目标绑定后进入 wasm 助记符派发。
fn numeric_submission(mnemonic: &str) -> FragmentSubmission {
    let groups = [CompilerSourceGroup::single_source("demo", "demo", &app_source(mnemonic)).as_entry()];
    let program = ValkyrieCompiler::default()
        .compile_source_groups_to_program_with_host_bindings(&groups, &[], "wasm")
        .expect("i32 算术源码必须完成 Compiler 分析");
    let fragment = program.canonical().linked.fragments.keys().next().cloned().expect("目标程序必须保留片段身份");
    let roots = program.canonical().mir.functions.keys().copied().collect::<Vec<_>>();
    let plan = TargetExecutionPlan::from_compiled_program(&program, &fragment, nyar::TheoryBundle::default(), &roots)
        .expect("`[wasm_opcode]` 声明必须进入目标计划导入表");
    nyar_emitter::testing::fragment_submission_from_plan(plan)
}

/// 源码 i32 `+` 经 provider 绑定后直接编码为 `i32.add`（0x6A）。
#[test]
fn source_i32_add_lowers_to_wasm_opcode_binding() {
    let submission = numeric_submission("i32.add");
    let module = nyar_emitter::testing::lower_fragment_to_wasm_mir_module(&submission, "main").expect("数值绑定产物必须完成 lowering");
    let bytes = module.to_bytes().expect("数值绑定产物必须可编码");
    assert!(bytes.contains(&0x6A), "i32 `+` 的 `[wasm_opcode(\"i32.add\")]` 绑定必须编码为 0x6A");
}

/// 无符号助记符必须由 backend 表编码，不得退化为有符号指令。
#[test]
fn unsigned_wasm_opcode_mnemonic_lowers_to_unsigned_instruction() {
    let submission = numeric_submission("i32.div_u");
    let module = nyar_emitter::testing::lower_fragment_to_wasm_mir_module(&submission, "main").expect("无符号数值绑定产物必须完成 lowering");
    let bytes = module.to_bytes().expect("无符号数值绑定产物必须可编码");
    assert!(bytes.contains(&0x6E), "`i32.div_u` 必须编码为无符号除法 0x6E");
}

/// 未知助记符不得回落到默认指令或宿主导入，必须返回结构化 backend error。
#[test]
fn unknown_wasm_opcode_mnemonic_fails_structurally() {
    let submission = numeric_submission("i32.unbound_add");
    let error = nyar_emitter::testing::lower_fragment_to_wasm_mir_module(&submission, "main")
        .expect_err("未知 `wasm_opcode` 助记符必须结构化失败");
    let message = error.to_string();
    assert!(message.contains("wasm_opcode"), "{message}");
    assert!(message.contains("i32.unbound_add"), "{message}");
    assert!(message.contains("function="), "错误必须带函数符号: {message}");
}

/// 助记符 arity 与实参数量不符时不得发射指令，必须返回结构化 backend error。
#[test]
fn wasm_opcode_arity_mismatch_fails_structurally() {
    let source = r#"
[primitive("core::primitive::i32")]
structure i32 { }

[wasm_opcode("i32.neg")]
abstract micro i32_neg(a: i32, b: i32): i32

micro subtract(a: i32, b: i32) -> i32 {
    return i32_neg(a, b)
}

[main]
micro main() -> i32 {
    return subtract(2, 3)
}
"#;
    let groups = [CompilerSourceGroup::single_source("demo", "demo", source).as_entry()];
    let program = ValkyrieCompiler::default()
        .compile_source_groups_to_program_with_host_bindings(&groups, &[], "wasm")
        .expect("i32 数值源码必须完成 Compiler 分析");
    let fragment = program.canonical().linked.fragments.keys().next().cloned().expect("目标程序必须保留片段身份");
    let roots = program.canonical().mir.functions.keys().copied().collect::<Vec<_>>();
    let plan = TargetExecutionPlan::from_compiled_program(&program, &fragment, nyar::TheoryBundle::default(), &roots)
        .expect("`[wasm_opcode]` 声明必须进入目标计划导入表");
    let submission = nyar_emitter::testing::fragment_submission_from_plan(plan);
    let error = nyar_emitter::testing::lower_fragment_to_wasm_mir_module(&submission, "main")
        .expect_err("助记符 arity 与实参数量不符必须结构化失败");
    let message = error.to_string();
    assert!(message.contains("i32.neg"), "{message}");
    assert!(message.contains("expected=1"), "错误必须带期望 arity: {message}");
    assert!(message.contains("actual=2"), "错误必须带实际实参数: {message}");
    assert!(message.contains("function="), "错误必须带函数符号: {message}");
}
