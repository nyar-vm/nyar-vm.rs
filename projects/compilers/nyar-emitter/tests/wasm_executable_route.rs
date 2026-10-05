use std::{
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use nyar::{
    CanonicalTarget, ClrSuspendStrategy, ExternalCallArgument, ExternalCallEdge, ExternalImportLink, HostProjectionBoundary, Identifier,
    QualifiedName, WitnessCallEdge,
};
use nyar_emitter::{FragmentSubmission, testing::lower_fragment_to_wasm_module};
use nyar_language::{ValkyrieCompiler, assemble_fragment_submission, plan_artifacts_from_build_output};

fn source_submission(source: &str) -> FragmentSubmission {
    let output = ValkyrieCompiler::default().compile_source_to_build_output(source).expect("当前源码必须完成 Compiler 分析");
    let target = CanonicalTarget::parse("node").expect("正式 Node target");
    let plan = plan_artifacts_from_build_output(&output, target, ClrSuspendStrategy::default()).expect("Compiler 目标规划");
    assert_eq!(plan.partitions.len(), 1);
    assemble_fragment_submission(&output, &plan, 0).expect("当前函数体进入目标输入")
}

fn run_in_node(bytes: &[u8], export: &str) -> serde_json::Value {
    let script = r#"
        import fs from 'node:fs';
        import assert from 'node:assert/strict';
        const input = JSON.parse(fs.readFileSync(0, 'utf8'));
        const bytes = Uint8Array.from(input.bytes);
        assert.equal(WebAssembly.validate(bytes), true, '当前产物必须通过验证');
        const recorded = [];
        const env = {
            emit_byte(value) { recorded.push(value); },
            const_utf8() { throw new Error('当前整数源码不得调用文本导入'); }
        };
        for (const field of ['utf8_trim', 'utf8_length', 'utf8_concat', 'utf8_starts_with', 'utf8_ends_with', 'utf8_contains', 'utf8_equals', 'utf8_replace', 'utf8_index_of', 'utf8_slice', 'utf8_to_lower', 'utf8_to_upper']) {
            env[field] = () => { throw new Error('整数源码不得调用文本协议: ' + field); };
        }
        const { instance } = await WebAssembly.instantiate(bytes, { env });
        assert.equal(typeof instance.exports[input.export], 'function');
        const result = instance.exports[input.export]();
        process.stdout.write(JSON.stringify({ result: String(result), recorded }));
    "#;
    let mut child = Command::new("node")
        .args(["--max-old-space-size=128", "--input-type=module", "-e", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("合同测试需要 Node，不得跳过运行");
    child.stdin.take().unwrap().write_all(&serde_json::to_vec(&serde_json::json!({ "bytes": bytes, "export": export })).unwrap()).unwrap();
    let started = Instant::now();
    while child.try_wait().expect("查询本测试 Node 子进程").is_none() {
        if started.elapsed() >= Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Node 执行超过十秒，测试失败，不保留失控子进程");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().expect("等待真实产物执行");
    assert!(output.status.success(), "Node verify/load/执行失败: {}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).expect("真实结果必须可比较")
}

#[test]
fn wasm_source_body_is_not_replaced_by_string_output_edges() {
    let mut submission = source_submission("[main] micro entry() -> i32 { return 23 }");
    let entry = submission.entry_operation.clone().expect("源码入口");
    let output = QualifiedName::new(vec![Identifier::new("external"), Identifier::new("output")]);
    submission
        .external_import_links
        .insert(output.clone(), ExternalImportLink::host(Some(Identifier::new("wasm")), vec!["env".into(), "emit_byte".into()]));
    submission.external_call_edges.push(ExternalCallEdge::new(entry, output, vec![ExternalCallArgument::StringLiteral("伪造输出".into())]));
    let (module, _) = lower_fragment_to_wasm_module(&submission, HostProjectionBoundary::WasmJsGlue).expect("真实函数体不能切换到调用边重放器");
    let result = run_in_node(&module.to_bytes().expect("当前产物编码"), "main");
    assert_eq!(result, serde_json::json!({ "result": "23", "recorded": [] }));
}

#[test]
fn wasm_source_calls_keep_the_executable_route() {
    let submission =
        source_submission("micro identity(value: i32) -> i32 { return value } [main] micro entry() -> i32 { return identity(37) }");
    let (module, _) = lower_fragment_to_wasm_module(&submission, HostProjectionBoundary::WasmJsGlue).expect("普通调用必须编码真实函数体");
    let result = run_in_node(&module.to_bytes().expect("当前产物编码"), "main");
    assert_eq!(result, serde_json::json!({ "result": "37", "recorded": [] }));
}

#[test]
fn wasm_source_i64_return_preserves_the_declared_width() {
    for source in [
        "[main] micro entry() -> i64 { return 4294967297 }",
        "micro identity(value: i64) -> i64 { return value } [main] micro entry() -> i64 { return identity(4294967297) }",
    ] {
        let submission = source_submission(source);
        let (module, _) = lower_fragment_to_wasm_module(&submission, HostProjectionBoundary::WasmJsGlue).expect("宽整数源码必须编码真实结果");
        assert_eq!(run_in_node(&module.to_bytes().unwrap(), "main"), serde_json::json!({ "result": "4294967297", "recorded": [] }));
    }
}

#[test]
fn wasm_loop_block_arguments_use_parallel_assignment() {
    use nyar_emitter::{
        contracts::{Block, BlockRef, Instruction, InstructionKind, Operand, Terminator, ValueRef},
        executable_provider::MirFunctionMapProvider,
    };
    use std::{collections::BTreeMap, sync::Arc};
    let mut submission = source_submission("[main] micro entry() -> i32 { return 23 }");
    let entry = submission.entry_operation.clone().unwrap();
    let provider = submission.executable.as_ref().unwrap();
    let mut function = provider.get_function(&entry).unwrap().function.clone();
    let integer = nyar::NyarType::Integer32 { signed: true };
    function.value_types = BTreeMap::from([
        (ValueRef(0), integer.clone()),
        (ValueRef(1), integer.clone()),
        (ValueRef(2), nyar::NyarType::Boolean),
        (ValueRef(3), nyar::NyarType::Boolean),
        (ValueRef(4), integer.clone()),
        (ValueRef(5), integer.clone()),
        (ValueRef(6), nyar::NyarType::Boolean),
    ]);
    let literals = [
        (ValueRef(4), nyar_emitter::contracts::Constant::Int(17), integer.clone()),
        (ValueRef(5), nyar_emitter::contracts::Constant::Int(29), integer),
        (ValueRef(6), nyar_emitter::contracts::Constant::Bool(true), nyar::NyarType::Boolean),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (result, constant, ty))| {
        let mut instruction = Instruction::from_kind(InstructionKind::LoadConstant { constant, ty: Some(ty) });
        instruction.id = nyar_types::InstructionId::from_index(index as u32).unwrap();
        instruction.results = vec![result];
        instruction
    })
    .collect();
    let mut condition = Instruction::from_kind(InstructionKind::LoadConstant {
        constant: nyar_emitter::contracts::Constant::Bool(false),
        ty: Some(nyar::NyarType::Boolean),
    });
    condition.results = vec![ValueRef(3)];
    condition.id = nyar_types::InstructionId::from_index(3).unwrap();
    function.blocks = vec![
        Block {
            id: BlockRef(0),
            label: "entry".into(),
            parameters: vec![],
            instructions: literals,
            terminator: Terminator::Jump {
                target: BlockRef(1),
                arguments: vec![Operand::Value(ValueRef(4)), Operand::Value(ValueRef(5)), Operand::Value(ValueRef(6))],
            },
        },
        Block {
            id: BlockRef(1),
            label: "loop".into(),
            parameters: vec![ValueRef(0), ValueRef(1), ValueRef(2)],
            instructions: vec![],
            terminator: Terminator::Branch { condition: Operand::Value(ValueRef(2)), then_target: BlockRef(2), else_target: BlockRef(3) },
        },
        Block {
            id: BlockRef(2),
            label: "swap".into(),
            parameters: vec![],
            instructions: vec![condition],
            terminator: Terminator::Jump {
                target: BlockRef(1),
                arguments: vec![Operand::Value(ValueRef(1)), Operand::Value(ValueRef(0)), Operand::Value(ValueRef(3))],
            },
        },
        Block {
            id: BlockRef(3),
            label: "done".into(),
            parameters: vec![],
            instructions: vec![],
            terminator: Terminator::Return { value: Some(Operand::Value(ValueRef(1))) },
        },
    ];
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(entry, function)]))));
    let (module, _) = lower_fragment_to_wasm_module(&submission, HostProjectionBoundary::WasmJsGlue).expect("已验证 CFG 必须保持并行复制");
    assert_eq!(run_in_node(&module.to_bytes().unwrap(), "main"), serde_json::json!({ "result": "17", "recorded": [] }));
}

#[test]
fn wasm_missing_executable_never_synthesizes_an_empty_entry() {
    for boundary in [HostProjectionBoundary::WasmJsGlue, HostProjectionBoundary::WasiComponent] {
        let error = lower_fragment_to_wasm_module(&FragmentSubmission::default(), boundary).expect_err("缺函数体不能生成成功空壳");
        assert!(error.to_string().contains("Compiler-owned executable functions"), "{error}");
    }
}

#[test]
fn wasm_witness_summary_never_manufactures_a_method_body() {
    let mut submission = source_submission("[main] micro entry() -> i32 { return 23 }");
    submission.witness_calls.push(WitnessCallEdge {
        trait_name: "DeclaredTrait".into(),
        type_name: "DeclaredType".into(),
        method_index: 0,
        print_result: false,
    });
    for boundary in [HostProjectionBoundary::WasmJsGlue, HostProjectionBoundary::WasiComponent] {
        let error = lower_fragment_to_wasm_module(&submission, boundary).expect_err("witness 摘要不是可执行分派合同");
        assert!(error.to_string().contains("Compiler-resolved executable dispatch"), "{error}");
    }
}

#[test]
fn wasm_declared_entry_must_have_an_exact_executable_body() {
    let mut submission = source_submission("[main] micro entry() -> i32 { return 23 }");
    submission.entry_operation = Some(QualifiedName::new(vec![Identifier::new("other"), Identifier::new("entry")]));
    let error = lower_fragment_to_wasm_module(&submission, HostProjectionBoundary::WasmJsGlue).expect_err("入口缺体不能借用同名函数");
    assert!(error.to_string().contains("BPHYS008") && error.to_string().contains("no exact semantic function"), "{error}");
}

#[test]
fn wasm_missing_return_value_fails_at_the_semantic_boundary() {
    use nyar_emitter::{contracts::Terminator, executable_provider::MirFunctionMapProvider};
    use std::{collections::BTreeMap, sync::Arc};
    let mut submission = source_submission("[main] micro entry() -> i32 { return 23 }");
    let entry = submission.entry_operation.clone().unwrap();
    let mut function = submission.executable.as_ref().unwrap().get_function(&entry).unwrap().function.clone();
    function.blocks[0].terminator = Terminator::Return { value: None };
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(entry, function)]))));
    let error = lower_fragment_to_wasm_module(&submission, HostProjectionBoundary::WasmJsGlue).expect_err("缺返回值不能补零");
    assert!(error.to_string().contains("SMIR007") && error.to_string().contains("non-unit return"), "{error}");
}
