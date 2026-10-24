//! `TargetExecutionPlan` 冻结目标指令配方后，backend 只消费计划事实。
//!
//! 正向：计划里的 `[wasm_opcode]` 配方（arity、载体、opcode）与 backend 编码一致。
//! 反向：声明与冻结配方在结果槽/arity 上不一致，或助记符没有配方时，必须结构化
//! 失败，不得由 backend 补默认值或静默改槽。

use nyar_emitter::{
    FragmentSubmission, TargetExecutionPlan,
    target_instruction::{TargetInstructionEncoding, TargetOperandRecipe, TargetOperandType, TargetOpcodeBinding},
};
use nyar_language::{CompilerSourceGroup, ValkyrieCompiler};

fn plan_from_source(source: &str) -> Result<TargetExecutionPlan, miette::Report> {
    let groups = [CompilerSourceGroup::single_source("demo", "demo", source).as_entry()];
    let program = ValkyrieCompiler::default().compile_source_groups_to_program_with_host_bindings(&groups, &[], "wasm")?;
    let fragment = program.canonical().linked.fragments.keys().next().cloned().expect("目标程序必须保留片段身份");
    let roots = program.canonical().mir.functions.keys().copied().collect::<Vec<_>>();
    TargetExecutionPlan::from_compiled_program(&program, &fragment, nyar::TheoryBundle::default(), &roots)
}

fn numeric_source(mnemonic: &str, result: &str) -> String {
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
abstract micro i32_add(a: i32, b: i32): {result}

micro add(a: i32, b: i32) -> {result} {{
    return a + b
}}

[main]
micro main() -> {result} {{
    return add(2, 3)
}}
"#
    )
}

fn direct_source(return_type: &str) -> String {
    format!(
        r#"
[primitive("core::primitive::i32")]
structure i32 {{ }}

[primitive("core::primitive::i64")]
structure i64 {{ }}

[wasm_opcode("i32.add")]
abstract micro i32_add(a: i32, b: i32): {return_type}

[main]
micro main() -> {return_type} {{
    return i32_add(2, 3)
}}
"#
    )
}

fn lower(submission: &FragmentSubmission) -> Result<Vec<u8>, miette::Report> {
    nyar_emitter::testing::lower_fragment_to_wasm_mir_module(submission, "main")?
        .to_bytes()
        .map_err(|error| miette::miette!("{error:?}"))
}

/// 计划冻结的 `i32.add` 配方正好是 backend 编码的指令，backend 不按调用的
/// `CanonicalOperation` 或名字重新选择 arity/载体/opcode。
#[test]
fn frozen_wasm_numeric_recipe_is_consumed_without_backend_inference() {
    let plan = plan_from_source(&numeric_source("i32.add", "i32")).expect("兼容签名必须生成目标计划");
    assert_eq!(plan.imports().len(), 1, "只应有一个目标指令绑定导入");
    let import = plan.imports().values().next().expect("导入必须存在");
    let Some(TargetOpcodeBinding::Frozen(instruction)) = import.target_instruction
    else {
        panic!("`i32.add` 必须在计划里冻结成完整配方");
    };
    assert_eq!(instruction.arity, 2);
    assert_eq!(instruction.operand_type, TargetOperandType::I32);
    assert_eq!(instruction.result_type, TargetOperandType::I32);
    assert_eq!(instruction.recipe, TargetOperandRecipe::Operands);
    assert_eq!(instruction.encoding, TargetInstructionEncoding::Wasm(0x6A));

    let submission = nyar_emitter::testing::fragment_submission_from_plan(plan);
    let bytes = lower(&submission).expect("已冻结配方必须可编码");
    assert!(bytes.contains(&0x6A), "backend 必须编码计划冻结的 0x6A，而不是重新推导");
}

/// 声明的返回槽载体与冻结结果载体不一致时，计划构造必须结构化失败。
#[test]
fn declared_result_slot_mismatch_fails_plan_construction() {
    let error = plan_from_source(&direct_source("i64")).expect_err("返回 i64 与冻结的 i32 结果槽不符必须失败");
    let message = error.to_string();
    assert!(message.contains("i32.add"), "{message}");
    assert!(message.contains("结果槽"), "错误必须指出结果槽不符: {message}");
    assert!(message.contains("expected=I32"), "错误必须带冻结载体: {message}");
    assert!(message.contains("actual=I64"), "错误必须带声明载体: {message}");
    assert!(message.contains("function="), "错误必须带函数符号: {message}");
}

/// 声明 arity 与冻结配方 arity 不一致时，计划构造必须结构化失败。
#[test]
fn declared_arity_mismatch_fails_plan_construction() {
    let source = r#"
[primitive("core::primitive::i32")]
structure i32 { }

[wasm_opcode("i32.neg")]
abstract micro i32_neg(a: i32, b: i32): i32

[main]
micro main() -> i32 {
    return i32_neg(2, 3)
}
"#;
    let error = plan_from_source(source).expect_err("两参数声明与冻结的一元配方不符必须失败");
    let message = error.to_string();
    assert!(message.contains("i32.neg"), "{message}");
    assert!(message.contains("expected=1"), "错误必须带冻结 arity: {message}");
    assert!(message.contains("actual=2"), "错误必须带声明参数数: {message}");
}

/// 助记符没有可用配方时计划保留 `Unfrozen`，backend 必须结构化失败而不是默认成功。
#[test]
fn unfrozen_wasm_opcode_recipe_fails_structurally() {
    let plan = plan_from_source(&numeric_source("i32.unbound_add", "i32")).expect("缺配方只应在编码阶段失败");
    let binding = plan.imports().values().next().and_then(|import| import.target_instruction);
    assert_eq!(binding, Some(TargetOpcodeBinding::Unfrozen), "未知助记符不得被冻结为任何指令");

    let submission = nyar_emitter::testing::fragment_submission_from_plan(plan);
    let error = lower(&submission).expect_err("缺配方不得编码为默认指令");
    let message = error.to_string();
    assert!(message.contains("i32.unbound_add"), "{message}");
    assert!(message.contains("function="), "错误必须带函数符号: {message}");
}
