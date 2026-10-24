//! 数组长度结构操作的前端身份只来自源码声明。
//!
//! `core/source/array/length.v` 中的
//! `[intrinsic("builtin.array.length")] micro length<T>(array: [T]): usize { }`
//! 是唯一前端来源；删除该声明后调用必须结构化失败，前端不得合成候选。

use rusty_valkyrie::{
    MirDiagnostic, MirLowerer, MirOperation, ValkyrieCompiler,
    types::hir::{HirExpr, HirExprKind, HirFunction, ValkyrieType},
    HirError,
};
use nyar_types::IntrinsicId;

/// 数值原始类型名属于 `core/primitive` 的声明族；未声明时 canonical 以 CAN003 拒绝。
const PRIMITIVES: &str = "[primitive(\"core::primitive::usize\")] structure usize { } \
    [primitive(\"core::primitive::i32\")] structure i32 { } ";

/// 与 `valkyrie.v` core 声明逐字对齐的唯一前端来源。
const ARRAY_LENGTH_INTRINSIC_DECLARATION: &str = "[intrinsic(\"builtin.array.length\")] micro length<T>(array: [T]): usize { } ";

/// 与 `asgard.router` 相同的调用形式：`length(segments)`。
const PROBE: &str = "micro probe(values: [i32]) -> usize { return length(values) }";

/// 语句位置的调用没有返回注解可补，是天然的“无上下文类型事实”样本。
const STATEMENT_PROBE: &str = "micro probe(values: [i32]) { length(values) }";

fn probe_source(intrinsic_declaration: Option<&str>) -> String {
    format!("{PRIMITIVES}{}{PROBE}", intrinsic_declaration.unwrap_or(""))
}

fn find_function<'a>(module: &'a rusty_valkyrie::types::hir::HirModule, name: &str) -> &'a HirFunction {
    module.functions.iter().find(|function| function.name.as_str() == name).unwrap_or_else(|| panic!("缺少 `{name}` 声明"))
}

fn probe_return_call(function: &HirFunction) -> &HirExpr {
    // 尾部 `return` 是块尾表达式（`body.expr`），不是普通语句。
    let expression = function.body.expr.as_deref().unwrap_or_else(|| panic!("expected trailing return expression: {:?}", function.body));
    let HirExprKind::Return(Some(expression)) = &expression.kind
    else {
        panic!("expected return expression: {expression:?}");
    };
    expression.as_ref()
}

#[test]
fn declared_array_length_intrinsic_lowers_to_array_len_identity() {
    let compiler = ValkyrieCompiler::default();
    let hir = compiler.compile_source(&probe_source(Some(ARRAY_LENGTH_INTRINSIC_DECLARATION))).expect("源码内声明的数组长度结构操作必须解析");

    let expression = probe_return_call(find_function(&hir, "probe"));
    let HirExprKind::Call { callee, args, resolved } = &expression.kind
    else {
        panic!("expected length call, got {expression:?}");
    };
    assert_eq!(args.len(), 1, "receiver must be the only argument: {args:?}");
    let resolved = resolved.as_ref().expect("length call must resolve through the declared intrinsic candidate");
    assert_eq!(resolved.intrinsic, Some(IntrinsicId::ArrayLen), "数组长度身份必须来自声明冻结的 intrinsic，而不是调用点名字");
    assert!(
        matches!(resolved.return_type, ValkyrieType::Nominal { .. }),
        "声明可见的 usize 必须已冻结为 Nominal identity，got {:?}",
        resolved.return_type
    );
    let declared_return_type = resolved.return_type.clone();
    let HirExprKind::Path(path) = &callee.kind
    else {
        panic!("expected bare length callee, got {callee:?}");
    };
    assert_eq!(path.parts().len(), 1, "callee must stay the bare selection name: {path}");

    let semantic = MirLowerer::lower_module_semantic(&hir);
    assert!(semantic.diagnostics.is_empty(), "声明齐全时 lowering 不得产生诊断: {:?}", semantic.diagnostics);
    let mut array_length_result_types = Vec::new();
    for function in &semantic.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                if matches!(instruction.kind, MirOperation::ArrayLength { .. }) {
                    assert_eq!(instruction.results.len(), 1, "ArrayLength 必须绑定唯一结果槽");
                    array_length_result_types.push(function.value_types.get(&instruction.results[0]).cloned());
                }
            }
        }
    }
    // 结果类型必须消费声明返回类型本身，不得按名字另造 `Named("usize")` 形成双 TypeId。
    assert_eq!(array_length_result_types, vec![Some(declared_return_type)], "ArrayLength 结果类型必须来自声明身份，不得回落为名字类型");
}

#[test]
fn missing_array_length_return_type_fact_fails_structurally() {
    let mut hir = ValkyrieCompiler::default()
        .compile_source(&format!("{PRIMITIVES}{ARRAY_LENGTH_INTRINSIC_DECLARATION}{STATEMENT_PROBE}"))
        .expect("声明齐全时 HIR 必须先成立");
    let probe = hir.functions.iter_mut().find(|function| function.name.as_str() == "probe").expect("probe 声明");
    let expression = probe.body.expr.as_deref_mut().expect("尾部 length 调用表达式");
    let HirExprKind::Call { resolved: Some(resolved), .. } = &mut expression.kind
    else {
        panic!("预期已解析的 length 调用: {expression:?}");
    };
    // 移除调用点唯一的返回类型事实；语句位置没有注解可以补回。
    resolved.return_type = ValkyrieType::Unit;

    let semantic = MirLowerer::lower_module_semantic(&hir);
    assert!(
        semantic
            .diagnostics
            .iter()
            .any(|diagnostic| { matches!(diagnostic, MirDiagnostic::UnresolvedValueType { context } if context.contains("array length")) }),
        "缺失结果类型事实必须结构化失败，实际诊断: {:?}",
        semantic.diagnostics
    );
    let emitted_array_length = semantic
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .any(|instruction| matches!(instruction.kind, MirOperation::ArrayLength { .. }));
    assert!(!emitted_array_length, "事实缺失时不得按名字造 usize 继续发射 ArrayLength");
    assert!(
        semantic
            .functions
            .iter()
            .all(|function| function.value_types.values().all(|ty| !matches!(ty, ValkyrieType::Named(name) if name.as_str() == "usize"))),
        "不得回落成 Named(\"usize\")"
    );
}

#[test]
fn undeclared_array_length_intrinsic_fails_without_frontend_synthesis() {
    let error =
        ValkyrieCompiler::default().compile_source_to_program(&probe_source(None)).expect_err("缺少 intrinsic 声明时数组长度调用不得成功");
    let rendered = format!("{error:?}");
    assert!(matches!(error, HirError::UnresolvedCall { .. }), "缺少声明必须结构化失败为 UnresolvedCall: {rendered}");
    assert!(rendered.contains("length"), "失败必须指出未解析的 length 调用: {rendered}");
}
