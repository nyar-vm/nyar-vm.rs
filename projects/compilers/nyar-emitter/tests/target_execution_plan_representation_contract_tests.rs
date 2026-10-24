use miette::Result;
use nyar::{NamePath, QualifiedName};
use nyar_emitter::{
    CompiledProgram, FragmentSubmission, TargetExecutionPlan,
    contracts::{InstructionKind, Operand},
};
use nyar_types::{ItemInstanceId, layout_choice::{InvokeLowering, ValueRepresentation}};

fn source_program() -> CompiledProgram {
    source_program_from(
        "micro identity(value: i32) -> i32 { return value } \
         micro entry(value: i32) -> i32 { return identity(value) }",
    )
}

/// 与生产 `core/primitive` 同构的最小前导。
///
/// 数值 primitive 名不是内建，必须由可见的 `structure` 声明冻结为声明身份；
/// 缺少声明时 `i32`/`i64` 只残留待绑定的 `Named`，会被 canonical 以 CAN003 拒绝。
const PRIMITIVE_DECLARATIONS: &str = r#"[primitive("core::primitive::i32")]
structure i32 { }

[primitive("core::primitive::i64")]
structure i64 { }
"#;

fn source_program_from(source: &str) -> CompiledProgram {
    use nyar_types::pipeline::RepresentationPlanStage;
    let source = format!("{PRIMITIVE_DECLARATIONS}\n{source}");
    let hir = rusty_valkyrie::ValkyrieCompiler::default().compile_source(&source).expect("单测源码必须完成 HIR 分析");
    let mir = rusty_valkyrie::MirLowerer::lower_module_semantic(&hir);
    let canonical =
        rusty_valkyrie::compile_pipeline::canonical_program_from_semantic_mir(&mir).expect("单测 MIR 必须满足 Canonical 合同");
    let representation =
        rusty_valkyrie::compile_pipeline::CanonicalRepresentationPlanner.plan(&canonical).expect("单测 Canonical 必须形成表示计划");
    CompiledProgram::new(canonical, representation).expect("单测表示输入必须一致，不代表生产流水线验收")
}

fn prepare(program: &CompiledProgram) -> Result<TargetExecutionPlan> {
    let roots = program.canonical().mir.functions.keys().copied().collect::<Vec<_>>();
    let fragment = program.canonical().linked.fragments.keys().next().expect("测试程序具有片段");
    TargetExecutionPlan::from_compiled_program(program, fragment, nyar::TheoryBundle::default(), &roots)
}

#[test]
fn source_specialized_values_and_direct_calls_prepare() {
    let program = source_program();
    let plan = prepare(&program).expect("已有完整标量与直接调用合同必须可表达");
    assert_eq!(plan.testing_functions().len(), program.canonical().mir.functions.len());
}

#[test]
fn unknown_fragment_identity_fails_before_backend_preparation() {
    let program = source_program();
    let missing = nyar::Identifier::new("missing_fragment");
    let error = TargetExecutionPlan::from_compiled_program(&program, &missing, nyar::TheoryBundle::default(), &[])
        .expect_err("片段必须来自同一 Canonical 程序，不能按根或顺序猜测");
    assert!(error.to_string().contains("不存在"), "{error}");
}

#[test]
fn unknown_root_identity_fails_before_function_projection() {
    let program = source_program();
    let unknown = ItemInstanceId::from_index(1000).expect("构造闭包外身份");
    assert!(!program.canonical().mir.functions.contains_key(&unknown));
    let fragment = program.canonical().linked.fragments.keys().next().expect("测试程序具有片段");
    let error = TargetExecutionPlan::from_compiled_program(&program, fragment, nyar::TheoryBundle::default(), &[unknown])
        .expect_err("闭包外根身份必须失败，不能改按名称寻找其他函数");
    assert!(error.to_string().contains("缺少 canonical 函数体"), "{error}");
}

#[test]
fn root_identity_is_not_resolved_again_from_abi_spelling() {
    let source = source_program();
    let root = *source.canonical().mir.functions.keys().next().expect("源码有函数");
    let mut canonical = source.canonical().clone();
    canonical.linked.callable_names.insert(root, QualifiedName::new(vec![nyar::Identifier::new("renamed_boundary")]));
    let program = CompiledProgram::new(canonical, source.representation().clone()).expect("ABI 名不改变已经验证的调用与表示身份");
    let fragment = program.canonical().linked.fragments.keys().next().expect("测试程序具有片段");
    let plan = TargetExecutionPlan::from_compiled_program(&program, fragment, nyar::TheoryBundle::default(), &[root])
        .expect("目标准备必须消费同一根身份");
    assert!(plan.testing_functions().contains_key(&root));
    assert_eq!(plan.testing_functions()[&root].symbol, "renamed_boundary");
}

#[test]
fn operator_and_builtin_spellings_cannot_bypass_call_identity_validation() {
    let program = source_program();
    for parts in [vec!["infix +"], vec!["builtin", "array", "push"]] {
        let mut plan = prepare(&program).expect("源码计划必须成功");
        let call = plan
            .testing_functions_mut()
            .values_mut()
            .flat_map(|function| &mut function.blocks)
            .flat_map(|block| &mut block.instructions)
            .find_map(|instruction| match &mut instruction.kind {
                InstructionKind::Call { callee, .. } => Some(callee),
                _ => None,
            })
            .expect("源码包含调用");
        *call = Operand::Symbol(NamePath::new(parts.iter().map(|part| nyar::Identifier::new(part)).collect()));
        let submission = FragmentSubmission::from_backend_plan(plan);
        let error = nyar_emitter::lowering::features::semantic_mir_contract::validate_submission(&submission)
            .expect_err("拼写属于 builtin 或 operator 也不能替代实例合同");
        assert_eq!(error.code, "SMIR003");
    }
}

#[test]
fn duplicate_abi_labels_preserve_wasm_export_and_call_targets() {
    let source = source_program_from(
        "micro identity(value: i32) -> i32 { return value } \
         micro entry(value: i32) -> i32 { return identity(17) }",
    );
    let mut canonical = source.canonical().clone();
    let exports = canonical.linked.callable_names.clone();
    for name in canonical.linked.callable_names.values_mut() {
        *name = QualifiedName::new(vec![nyar::Identifier::new("same_label")]);
    }
    let program = CompiledProgram::new(canonical, source.representation().clone()).expect("标签不改变语义合同");
    let mut plan = prepare(&program).expect("独立实例必须可准备");
    *plan.testing_wasm_export_names_mut() =
        exports.iter().map(|(instance, name)| (*instance, name.parts().last().expect("测试声明名称").as_str().to_owned())).collect();
    let submission = FragmentSubmission::from_backend_plan(plan);
    let (module, _) =
        nyar_emitter::lowering::backends::wasm::lower_fragment_to_wasm_module(&submission, nyar::HostProjectionBoundary::WasmJsGlue)
            .expect("Wasm 调用和导出只能沿实例编码");
    let directory = tempfile::tempdir().expect("测试目录");
    let artifact = directory.path().join("identity.wasm");
    std::fs::write(&artifact, module.to_bytes().expect("Wasm 编码成功")).expect("写入测试产物");
    let output = std::process::Command::new("node").args(["--input-type=module", "-e",
            "import {readFileSync} from 'node:fs'; const {instance}=await WebAssembly.instantiate(readFileSync(process.argv[1])); if(instance.exports.identity(41)!==41 || instance.exports.entry(41)!==17) throw new Error('wrong instance target');",
        ]).arg(&artifact).output().expect("Node 必须运行");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn scalar_library_exports_do_not_require_collection_name_or_layout_glue() {
    let program = source_program_from("micro identity(value: i64) -> i64 { return value }");
    let exports = program.canonical().linked.callable_names.clone();
    let mut plan = prepare(&program).expect("标量导出有完整实例合同");
    *plan.testing_wasm_export_names_mut() =
        exports.iter().map(|(instance, name)| (*instance, name.parts().last().expect("测试声明名称").as_str().to_owned())).collect();
    let submission = FragmentSubmission::from_backend_plan(plan);
    let (module, _) = nyar_emitter::lowering::backends::wasm::lower_fragment_to_wasm_module_for(
        &submission,
        nyar::HostProjectionBoundary::WasmJsGlue,
        nyar_emitter::nyar_backend_wasi::WasiPreview::Preview2,
        nyar_emitter::nyar_backend_wasi::WasmPackageKind::Library,
    )
    .expect("标量库不得要求 ArrayList 函数或字段布局");
    let section = module.sections.iter().find(|section| section.name.as_deref() == Some("nyar.library_invoke")).expect("必须提供明确导出 ABI");
    let metadata: serde_json::Value = serde_json::from_slice(&section.bytes).expect("ABI JSON 有效");
    assert_eq!(metadata["exports"]["identity"]["params"], serde_json::json!(["i64"]));
    assert_eq!(metadata["exports"]["identity"]["returns"], "i64");
    assert_eq!(metadata["glue"], serde_json::json!({}));
}

#[cfg(feature = "ovm-lane")]
#[test]
fn duplicate_abi_labels_do_not_redirect_nyar_calls_or_exports() {
    use nyar_types::{CanonicalCallee, CanonicalOperation};

    let source = source_program();
    let mut canonical = source.canonical().clone();
    let instances = canonical.mir.functions.keys().copied().collect::<Vec<_>>();
    assert_eq!(instances.len(), 2);
    for instance in &instances {
        canonical.linked.callable_names.insert(*instance, QualifiedName::new(vec![nyar::Identifier::new("same_label")]));
    }
    let program = CompiledProgram::new(canonical, source.representation().clone()).expect("诊断名不能改变实例身份");
    let mut plan = prepare(&program).expect("相同标签的函数必须保留独立实例");
    *plan.testing_exported_operations_mut() = instances.clone();
    *plan.testing_wasm_export_names_mut() =
        instances.iter().enumerate().map(|(index, instance)| (*instance, format!("export_{index}"))).collect();
    let submission = FragmentSubmission::from_backend_plan(plan);
    let module = nyar_emitter::lowering::backends::ovm::lower_fragment_to_nyar_module(&submission).expect("只允许沿已绑定实例发射");
    assert_eq!(module.functions.len(), 2);
    assert_eq!(module.exports.len(), 2);
    for (index, export) in module.exports.iter().enumerate() {
        assert_eq!(export.symbol_name, format!("export_{index}"));
        assert_eq!(export.function_index, index as i32);
    }
    let mut calls = Vec::new();
    let mut position = 0;
    while position < module.code_bytes.len() {
        let instruction = odin_bytecode::decode_at(&module.code_bytes, position);
        assert!(instruction.size > 0, "字节码必须可解码");
        if instruction.code == odin_bytecode::NyarHeadCode::Call {
            calls.push(instruction.operand1);
        }
        position += instruction.size as usize;
    }
    let callee = source
        .canonical()
        .mir
        .functions
        .values()
        .flat_map(|function| function.blocks.values())
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match &instruction.operation {
            CanonicalOperation::Invoke { callee: CanonicalCallee::Item(instance), .. } => Some(*instance),
            _ => None,
        })
        .expect("源码包含普通调用");
    assert_eq!(calls, vec![instances.iter().position(|instance| *instance == callee).expect("目标实例存在") as i32]);
}

#[test]
fn unsupported_value_choices_do_not_reuse_specialized_encoding() {
    let source = source_program();
    let identity = *source.representation().value_reps.keys().next().expect("源码应有 SSA 值");
    for representation in [ValueRepresentation::CompileTimeIdentity, ValueRepresentation::Reified, ValueRepresentation::ErasedBoxed] {
        let mut plan = source.representation().clone();
        plan.value_reps.insert(identity, representation.clone());
        let program = CompiledProgram::new(source.canonical().clone(), plan).expect("表示选择的可表达性属于目标准备边界");
        let error = prepare(&program).expect_err("不得忽略表示选择而重用标量编码");
        assert!(error.to_string().contains("目标私有载体合同"), "{error}");
        assert!(error.to_string().contains(&format!("{representation:?}")), "{error}");
    }
}

#[test]
fn unsupported_call_choices_do_not_reuse_direct_encoding() {
    let source = source_program();
    let instruction = *source.representation().invoke_lowerings.keys().next().expect("源码应有普通调用");
    for lowering in
        [InvokeLowering::TypedWitness, InvokeLowering::SharedOperationTable, InvokeLowering::Specialized, InvokeLowering::TypedReference]
    {
        let mut plan = source.representation().clone();
        plan.invoke_lowerings.insert(instruction, lowering.clone());
        let program = CompiledProgram::new(source.canonical().clone(), plan).expect("调用表示的可表达性属于目标准备边界");
        let error = prepare(&program).expect_err("不得忽略调用表示而重用直接调用编码");
        assert!(error.to_string().contains("目标私有调用合同"), "{error}");
        assert!(error.to_string().contains(&format!("{lowering:?}")), "{error}");
    }
}
