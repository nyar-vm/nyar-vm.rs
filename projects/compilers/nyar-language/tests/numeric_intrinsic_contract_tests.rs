//! 数值 intrinsic 从声明 identity 到目标指令的端到端合同。
//!
//! 正例：`[intrinsic("i32.add")]` 声明 + `imply i32` 运算符必须依次形成
//! `MirOperation::Numeric` → `CanonicalOperation::Numeric` → Nyar `I32Add` 指令。
//! 反例：没有目标实现来源的宽度名称必须无法解析为 `IntrinsicId`，也不得降低为数值指令。

use nyar_language::{
    MirLowerer, MirOperation, ValkyrieCompiler,
    valkyrie::{compile_pipeline::canonical_program_from_semantic_mir, mir::MirBlock},
};
use nyar_types::{CanonicalOperation, CanonicalProgram, IntrinsicId, NumericIntrinsic, NumericOp, NumericScalar};

const I32_SOURCE: &str = r#"
[primitive("core::primitive::i32")]
structure i32 { }

imply i32 {
    infix `+`(self, rhs: Self): Self {
        __i32_add(self, rhs)
    }
}

[intrinsic("i32.add")]
private micro __i32_add(lhs: i32, rhs: i32): i32 { }

[export(name: "add_one")]
micro add_one(value: i32) -> i32 {
    return value + 1
}
"#;

const U8_SOURCE: &str = r#"
[primitive("core::primitive::u8")]
structure u8 { }

imply u8 {
    infix `+`(self, rhs: Self): Self {
        __u8_add(self, rhs)
    }
}

[intrinsic("u8.add")]
private micro __u8_add(lhs: u8, rhs: u8): u8 { }

[export(name: "add_one")]
micro add_one(value: u8) -> u8 {
    return value + 1
}
"#;

fn operations(module: &nyar_language::MirModule) -> Vec<&MirOperation> {
    module
        .functions
        .iter()
        .flat_map(|function| function.blocks.iter())
        .flat_map(|block: &MirBlock| &block.instructions)
        .map(|instruction| instruction.operation())
        .collect()
}

fn i32_program() -> CanonicalProgram {
    let hir = ValkyrieCompiler::default().compile_source(I32_SOURCE).expect("`i32.add` 声明必须完成 HIR 分析");
    let mir = MirLowerer::lower_module_semantic(&hir);
    assert!(
        operations(&mir).iter().any(|operation| matches!(
            operation,
            MirOperation::Numeric { scalar: NumericScalar::I32, op: NumericOp::Add, .. }
        )),
        "`i32` 的 `+` 必须降低为 `MirOperation::Numeric`，实际操作为 {:?}",
        operations(&mir)
    );
    canonical_program_from_semantic_mir(&mir).expect("已绑定 adaptor 的数值 intrinsic 必须进入 canonical")
}

#[test]
fn supported_numeric_intrinsic_reaches_semantic_mir_and_canonical() {
    assert_eq!(
        IntrinsicId::resolve_from_segments(&["i32", "add"]),
        Some(IntrinsicId::Numeric(NumericIntrinsic::new(NumericScalar::I32, NumericOp::Add)))
    );

    let program = i32_program();
    assert!(
        program
            .mir
            .functions
            .values()
            .flat_map(|function| function.blocks.values())
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(
                instruction.operation,
                CanonicalOperation::Numeric { scalar: NumericScalar::I32, op: NumericOp::Add, .. }
            )),
        "`i32.add` 必须在 canonical 保留结构化数值身份"
    );
}

#[cfg(feature = "nyar-vm-lane")]
#[test]
fn supported_numeric_intrinsic_emits_real_target_instruction() {
    use nyar_types::pipeline::RepresentationPlanStage;

    let program = i32_program();
    let representation = nyar_language::valkyrie::compile_pipeline::CanonicalRepresentationPlanner
        .plan(&program)
        .expect("canonical 数值指令必须形成表示计划");
    let compiled = nyar_emitter::CompiledProgram::new(program, representation).expect("表示计划与 canonical 必须一致");
    let roots = compiled.canonical().mir.functions.keys().copied().collect::<Vec<_>>();
    let fragment = compiled.canonical().linked.fragments.keys().next().expect("测试程序具有片段");
    let plan = nyar_emitter::TargetExecutionPlan::from_compiled_program(&compiled, fragment, nyar::TheoryBundle::default(), &roots)
        .expect("数值 intrinsic 必须有目标私有合同");
    let submission = nyar_emitter::FragmentSubmission::from_backend_plan(plan);
    let module = nyar_emitter::lowering::backends::nyar_vm::lower_fragment_to_nyar_module(&submission)
        .expect("Nyar VM 后端必须承载已解析的数值 intrinsic");

    let mut position = 0;
    let mut emitted = Vec::new();
    while position < module.code_bytes.len() {
        let instruction = nyar_bytecode::decode_at(&module.code_bytes, position);
        assert!(instruction.size > 0, "字节码必须可解码");
        emitted.push(instruction.code);
        position += instruction.size as usize;
    }
    assert!(
        emitted.contains(&nyar_bytecode::NyarHeadCode::I32Add),
        "`i32` 的 `+` 必须发射真实 `I32Add` 指令，实际字节码为 {emitted:?}"
    );
}

#[test]
fn unsupported_numeric_intrinsic_is_not_an_identity_and_never_becomes_numeric() {
    assert_eq!(IntrinsicId::resolve_from_segments(&["u8", "add"]), None);

    let hir = ValkyrieCompiler::default().compile_source(U8_SOURCE).expect("`u8.add` 声明仍可完成 HIR 分析");
    let operator_body = hir.impls[0].methods[0].body.expr.as_ref().expect("运算符 body");
    let nyar_language::valkyrie::types::hir::HirExprKind::Call { resolved: Some(resolved), .. } = &operator_body.kind
    else {
        panic!("运算符 body 必须是已解析调用: {operator_body:?}");
    };
    assert_eq!(resolved.intrinsic, None, "没有目标实现来源的宽度不得获得 intrinsic 身份");

    let mir = MirLowerer::lower_module_semantic(&hir);
    assert!(
        !operations(&mir).iter().any(|operation| matches!(operation, MirOperation::Numeric { .. })),
        "没有目标实现来源的宽度不得降低为数值指令"
    );
}
