use std::panic::{AssertUnwindSafe, catch_unwind};

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
    let module = nyar_emitter::testing::lower_fragment_to_wasm_mir_module(&submission, "main");
    let bytes = module.to_bytes().expect("数值绑定产物必须可编码");
    assert!(bytes.contains(&0x6A), "i32 `+` 的 `[wasm_opcode(\"i32.add\")]` 绑定必须编码为 0x6A");
}

/// 未知助记符不得回落到默认指令或宿主导入。
#[test]
fn unknown_wasm_opcode_mnemonic_fails_structurally() {
    let submission = numeric_submission("i32.unbound_add");
    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _ = nyar_emitter::testing::lower_fragment_to_wasm_mir_module(&submission, "main");
    }))
    .expect_err("未知 `wasm_opcode` 助记符必须结构化失败");
    let message = panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
        .expect("panic 载荷必须是文本");
    assert!(message.contains("wasm_opcode"), "{message}");
    assert!(message.contains("i32.unbound_add"), "{message}");
}
