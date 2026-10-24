use rusty_valkyrie::{
    {
        hir::{
            overload::{OverloadCandidate, OverloadDomain, OverloadMatchKind, OverloadResolutionError, resolve_overload},
            type_relation::TypeRelationContext,
        },
        mir::AggregateFieldIndex,
        types::hir::{
            HirCallArgument, HirCallableDomain, HirExpr, HirExprKind, HirIdentifier, HirModule, HirResolvedCall, HirStatementKind,
            HirTraitBound, HirWhereConstraint,
            types::{HirParam, ValkyrieType},
        },
    },
    *,
};
use std::collections::{BTreeMap, BTreeSet};

use rusty_valkyrie::hir::overload::{
    enrich_sum_variant_operation_facts, extract_callable_name, infer_expr_type, infer_scrutinee_type, match_call_candidate,
    match_singleton_method_candidate, substitute_associated_equations, substitute_type_self, try_resolve_constructor,
    try_resolve_nullary_constructor,
};

fn test_binder(owner: u32, ordinal: u32) -> rusty_valkyrie::types::hir::GenericBinderId {
    rusty_valkyrie::types::hir::GenericBinderId { owner: nyar_types::ItemId::from_index(owner).unwrap(), ordinal }
}

fn test_generic(name: &str) -> ValkyrieType {
    ValkyrieType::Generic(rusty_valkyrie::types::hir::GenericType {
        binder: Some(test_binder(900, 0)),
        name: Identifier::new(name),
        kind: rusty_valkyrie::types::hir::HirKind::Type,
        bounds: vec![],
    })
}

#[test]
fn nominal_arguments_preserve_identity_and_substitute_associated_self() {
    let declaration = nyar_types::ItemId::from_index(41).unwrap();
    let subject = test_generic("I");
    let item = Identifier::new("Item");
    let original = ValkyrieType::Nominal {
        declaration,
        arguments: vec![ValkyrieType::Associated(Box::new(rusty_valkyrie::types::hir::AssociatedType {
            base: ValkyrieType::SelfType,
            name: item.clone(),
            type_arguments: vec![],
        }))],
    };
    let bound = substitute_type_self(&original, &subject);
    let equations = BTreeMap::from([(item, ValkyrieType::Utf8)]);
    assert_eq!(
        substitute_associated_equations(&bound, &subject, &equations),
        ValkyrieType::Nominal { declaration, arguments: vec![ValkyrieType::Utf8] }
    );
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn chained_generic_trait_result_resolves_declared_member() {
    let compiler = ValkyrieCompiler::default();
    let root = rusty_valkyrie::frontend::parse_source(
        r#"
class Parcel<T> { value: T }
imply Parcel<T> { micro take(self): T { return self.value } }
trait Cursor { type Item; micro next(self): Parcel<Item> }
micro collect<I, T>(iter: I): T where I: Cursor<Item = T> { return iter.next().take() }
"#,
    )
    .expect("解析链式调用");
    let hir = compiler.lower_root(&root).expect("绑定链式调用");
    compiler.validate_hir_semantic_contract(&hir).expect("链式调用必须绑定完整声明合同");
    let function = hir.functions.iter().find(|function| function.name.as_str() == "collect").unwrap();
    let HirExprKind::Return(Some(result)) = &function.body.expr.as_ref().unwrap().kind
    else {
        panic!("返回调用")
    };
    let HirExprKind::Call { callee, resolved: Some(call), .. } = &result.kind
    else {
        panic!("一次成员调用")
    };
    assert!(matches!(callee.kind, HirExprKind::FieldAccess { .. }));
    assert!(call.declaration.is_some());
    assert_eq!(call.return_type, function.return_type);
}

#[test]
fn nested_call_type_requires_the_selected_contract() {
    let compiler = ValkyrieCompiler::default();
    let mut hir = compiler
        .compile_source("micro produce() -> i64 { 7 } micro consume(value: i64) -> i64 { value } micro caller() -> i64 { consume(produce()) }")
        .expect("嵌套调用必须从源码完成声明选择");
    compiler.validate_hir_semantic_contract(&hir).expect("完整调用合同必须通过");
    let caller = hir.functions.iter_mut().find(|function| function.name.as_str() == "caller").unwrap();
    let HirExprKind::Call { args, .. } = &mut caller.body.expr.as_mut().unwrap().kind
    else {
        panic!("预期外层调用");
    };
    let nested = &mut args[0].value;
    // 该源码没有声明数值 primitive：`i64` 无声明身份，按 `Named` 冻结，不能折成内建标量
    // `Integer64`。内建标量形式只属于字面量推断与显式数值转换目标。
    let expected = Some(ValkyrieType::Named(Identifier::new("i64")));
    assert_eq!(infer_expr_type(nested, &BTreeMap::new()), expected);
    assert_eq!(infer_scrutinee_type(nested, &BTreeMap::new(), &AggregateFieldIndex::default(), &BTreeSet::new()), expected);
    let HirExprKind::Call { resolved, .. } = &mut nested.kind
    else {
        panic!("预期嵌套调用");
    };
    assert!(resolved.take().is_some());
    assert_eq!(infer_expr_type(nested, &BTreeMap::new()), None);
    assert_eq!(infer_scrutinee_type(nested, &BTreeMap::new(), &AggregateFieldIndex::default(), &BTreeSet::new()), None);
    assert!(compiler.validate_hir_semantic_contract(&hir).is_err());
}

#[test]
fn panic_and_format_names_require_source_callable_declarations() {
    let compiler = ValkyrieCompiler::default();
    for source in ["micro caller(value: i64) -> i64 { panic(value) }", "micro formatter(value: i64) -> i64 { format(value) }"] {
        let error = compiler.compile_source(source).expect_err("未声明名称不得获得合成 callable 合同");
        assert!(error.to_string().contains("SMIR003"), "应在 Semantic HIR 合同边界失败：{error}");
    }

    let hir = compiler
        .compile_source(
            r#"
micro panic(value: i64) -> i64 { value }
micro caller(value: i64) -> i64 { panic(value) }
"#,
        )
        .expect("显式声明应通过普通 callable resolver");
    let declaration = hir.functions.iter().find(|function| function.name.as_str() == "panic").unwrap();
    let caller = hir.functions.iter().find(|function| function.name.as_str() == "caller").unwrap();
    let HirExprKind::Call { resolved: Some(call), .. } = &caller.body.expr.as_ref().unwrap().kind
    else {
        panic!("显式声明必须得到正式调用合同");
    };
    assert_eq!(call.instance, declaration.instance);
}

#[test]
fn construction_type_requires_the_selected_contract() {
    let compiler = ValkyrieCompiler::default();
    let mut hir = compiler
        .compile_source("structure Parcel { value: i64 } micro create() -> Parcel { Parcel { value: 7 } }")
        .expect("构造必须从源码完成字段绑定与声明选择");
    compiler.validate_hir_semantic_contract(&hir).expect("完整构造合同必须通过");
    let parcel_declaration = hir.structs[0].constructor_declaration.expect("结构声明身份");
    let create = hir.functions.iter_mut().find(|function| function.name.as_str() == "create").unwrap();
    let construction = create.body.expr.as_mut().unwrap();
    let expected = Some(ValkyrieType::Nominal { declaration: parcel_declaration, arguments: vec![] });
    assert_eq!(infer_expr_type(construction, &BTreeMap::new()), expected);
    assert_eq!(infer_scrutinee_type(construction, &BTreeMap::new(), &AggregateFieldIndex::default(), &BTreeSet::new()), expected);
    let HirExprKind::Construct { resolved, .. } = &mut construction.kind
    else {
        panic!("预期结构构造");
    };
    assert!(resolved.take().is_some());
    assert_eq!(infer_expr_type(construction, &BTreeMap::new()), None);
    assert_eq!(infer_scrutinee_type(construction, &BTreeMap::new(), &AggregateFieldIndex::default(), &BTreeSet::new()), None);
    assert!(compiler.validate_hir_semantic_contract(&hir).is_err());
}

#[test]
fn selected_overload_preserves_all_declaration_facts_in_any_order() {
    let selected = declaration_candidate();
    let mut other = selected.clone();
    other.owner = Some(Identifier::new("OtherOwner"));
    other.trait_owner = Some(nyar_types::ItemId::from_index(910).unwrap());
    other.param_specs.clear();
    other.generic_binders.clear();
    other.where_constraints.clear();
    other.match_kind = OverloadMatchKind::Trait;

    for candidates in [vec![other.clone(), selected.clone()], vec![selected.clone(), other]] {
        assert_eq!(resolve_overload(&candidates).unwrap(), selected);
    }
}

#[test]
fn equal_rank_declarations_cannot_merge_after_erasing_facts() {
    let declaration = declaration_candidate();
    let mut different_owner = declaration.clone();
    different_owner.owner = Some(Identifier::new("OtherOwner"));
    let mut different_trait = declaration.clone();
    different_trait.trait_owner = Some(nyar_types::ItemId::from_index(910).unwrap());
    let mut different_receiver = declaration.clone();
    different_receiver.param_specs[0].name.name = Identifier::new("value");
    let mut different_binders = declaration.clone();
    different_binders.generic_binders.insert(test_binder(901, 0));
    let mut different_constraints = declaration.clone();
    different_constraints.where_constraints[0].bounds[0].trait_path = path("OtherBound");

    for (fact, other) in [
        ("owner", different_owner),
        ("trait_owner", different_trait),
        ("receiver", different_receiver),
        ("generic_binders", different_binders),
        ("where_constraints", different_constraints),
    ] {
        for candidates in [vec![declaration.clone(), other.clone()], vec![other, declaration.clone()]] {
            assert_eq!(
                resolve_overload(&candidates),
                Err(OverloadResolutionError::Ambiguous { candidates: vec![path("apply"), path("apply")] }),
                "{fact}"
            );
        }
    }
}

#[test]
fn repeated_complete_candidate_preserves_the_declaration() {
    let declaration = declaration_candidate();
    assert_eq!(resolve_overload(&[declaration.clone(), declaration.clone()]).unwrap(), declaration);
}

#[test]
fn empty_overload_set_fails_without_a_default_candidate() {
    assert_eq!(resolve_overload(&[]), Err(OverloadResolutionError::NoMatch));
}

#[test]
fn singleton_matching_preserves_the_complete_declared_signature() {
    let declaration = declaration_candidate();
    let matched = match_singleton_method_candidate(
        &declaration,
        &[],
        &TypeRelationContext::from_module(&HirModule::default()),
        &BTreeMap::new(),
        &AggregateFieldIndex::default(),
        &BTreeSet::new(),
        true,
    )
    .expect("singleton 匹配不得擦除 self 或声明合同");
    assert_eq!(matched, declaration);
}

#[test]
fn singleton_matching_rejects_an_argument_without_a_type_fact() {
    let declaration = declaration_candidate();
    let span = rusty_valkyrie::types::SourceSpan::new(SourceID::default(), 0, 0);
    let unknown = HirCallArgument::positional(HirExpr {
        kind: HirExprKind::Variable(HirIdentifier { name: Identifier::new("unknown"), shadow_index: 0, span: span.clone() }),
        span,
    });
    assert!(
        match_singleton_method_candidate(
            &declaration,
            &[unknown],
            &TypeRelationContext::from_module(&HirModule::default()),
            &BTreeMap::new(),
            &AggregateFieldIndex::default(),
            &BTreeSet::new(),
            true,
        )
        .is_none()
    );
}

fn declaration_candidate() -> OverloadCandidate {
    let mut declaration = candidate("apply", OverloadMatchKind::NominalExact);
    declaration.owner = Some(Identifier::new("Owner"));
    declaration.trait_owner = Some(nyar_types::ItemId::from_index(911).unwrap());
    let mut receiver = HirParam::default();
    receiver.name.name = Identifier::new("self");
    receiver.ty = ValkyrieType::Named(Identifier::new("Owner"));
    declaration.signature.params = vec![receiver.ty.clone()];
    declaration.where_constraints.push(HirWhereConstraint {
        target: ValkyrieType::Named(Identifier::new("Element")),
        bounds: vec![HirTraitBound { declaration: None, trait_path: path("Bound"), type_arguments: vec![], associated_types: vec![] }],
        span: receiver.name.span.clone(),
    });
    declaration.param_specs = vec![receiver];
    declaration.generic_binders.insert(test_binder(900, 0));
    declaration
}

fn candidate(name: &str, match_kind: OverloadMatchKind) -> OverloadCandidate {
    OverloadCandidate::new(path(name), OverloadDomain::Function, vec![], ValkyrieType::Unit, match_kind)
}

fn path(name: &str) -> NamePath {
    NamePath::new(vec![Identifier::new(name)])
}

fn resolve_constructor_contract(candidates: &[OverloadCandidate], actual: &[Option<ValkyrieType>]) -> Option<HirResolvedCall> {
    let mut locals = BTreeMap::new();
    let arguments = actual
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            let name = Identifier::new(&format!("argument_{index}"));
            let span = rusty_valkyrie::types::SourceSpan::new(SourceID::default(), 0, 0);
            if let Some(ty) = ty {
                locals.insert(name.to_string(), ty.clone());
            }
            HirExpr { kind: HirExprKind::Variable(HirIdentifier { name, shadow_index: 0, span: span.clone() }), span }
        })
        .collect::<Vec<_>>();
    try_resolve_constructor(
        &NamePath::new(vec![Identifier::new("Present")]),
        &arguments,
        candidates,
        &TypeRelationContext::from_module(&HirModule::default()),
        &locals,
        &AggregateFieldIndex::default(),
        &BTreeSet::new(),
        None,
    )
}

fn generic_constructor(owner: &str, parameters: Vec<ValkyrieType>) -> OverloadCandidate {
    OverloadCandidate::new_method(
        Identifier::new(owner),
        NamePath::new(vec![Identifier::new("Present")]),
        OverloadDomain::Constructor,
        parameters,
        ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new(owner))), vec![test_generic("T")]),
        OverloadMatchKind::Row,
    )
    .with_generic_binder(test_binder(900, 0))
}

#[test]
fn constructor_contract_instantiates_parameters_and_return_with_declared_owner() {
    let candidate = generic_constructor("Envelope", vec![test_generic("T")]);
    let resolved = resolve_constructor_contract(&[candidate], &[Some(ValkyrieType::Utf8)]).expect("声明 binder 应完成代入");
    assert_eq!(resolved.symbol, NamePath::new(vec![Identifier::new("Envelope"), Identifier::new("Present")]));
    assert_eq!(resolved.parameter_types, vec![ValkyrieType::Utf8]);
    assert_eq!(resolved.return_type, ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Envelope"))), vec![ValkyrieType::Utf8]));
    assert!(!resolved.has_receiver);
}

#[test]
fn constructor_contract_rejects_conflicting_repeated_binder() {
    let parameter = test_generic("T");
    let candidate = generic_constructor("Envelope", vec![parameter.clone(), parameter]);
    assert!(resolve_constructor_contract(&[candidate], &[Some(ValkyrieType::Utf8), Some(ValkyrieType::Boolean)]).is_none());
}

#[test]
fn constructor_contract_rejects_missing_argument_type_and_ambiguous_owner() {
    let parameter = test_generic("T");
    let candidates = vec![generic_constructor("Alpha", vec![parameter.clone()]), generic_constructor("Beta", vec![parameter])];
    assert!(resolve_constructor_contract(&candidates[..1], &[None]).is_none());
    assert!(resolve_constructor_contract(&candidates, &[None]).is_none());
    assert!(resolve_constructor_contract(&candidates, &[Some(ValkyrieType::Utf8)]).is_none());
}

#[test]
fn constructor_contract_rejects_partial_sum_identity_facts() {
    let mut candidate = generic_constructor("Envelope", vec![test_generic("T")]);
    candidate.sum_owner = nyar_types::ItemId::from_index(1);
    candidate.sum_variant = None;
    assert!(resolve_constructor_contract(&[candidate], &[Some(ValkyrieType::Utf8)]).is_none());
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn generic_aggregate_constructor_provides_field_context_to_nullary_variant() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 5230 });
    let hir = compiler
        .compile_source(
            r#"
unite Option<T> {
    Some { value: T },
    None,
}

class Promise<T> { value: Option<T>, done: bool, cancelled: bool }

trait Future {
    micro poll(self): bool
}

imply Promise<T>: Future {
    micro new(): Self {
        return Promise { value: None, done: false, cancelled: false }
    }
}
"#,
        )
        .expect("generic aggregate constructor should resolve its field context");
    assert!(hir.structs.iter().any(|item| item.name == Identifier::new("Promise")));
}

#[test]
fn option_none_turbofish_call_is_resolved_in_hir() {
    let source = r#"
namespace test;
unite Option<T> {
    Some { value: T },
    None,
}
micro option_none<T>() -> Option<T> {
    let empty: Option<T> = None
    return empty
}
micro take(flag: bool) -> Option<i64> {
    if flag {
        return option_none::<i64>()
    }
    let value: i64 = 1
    return Some(value)
}
"#;
    let root = rusty_valkyrie::frontend::parse_source(source).expect("parse option_none fixture");
    let take_ast = root
        .items
        .iter()
        .filter_map(|item| match item {
            rusty_valkyrie::frontend::ast::StatementNode::Micro(micro) if micro.name.name == "take" => Some(micro),
            _ => None,
        })
        .next()
        .expect("take micro in ast");
    let if_stmt = take_ast.body.statements.iter().find_map(|statement| match statement {
        rusty_valkyrie::frontend::ast::Statement::Expression(expr)
            if matches!(expr.expr, rusty_valkyrie::frontend::ast::TermExpression::If { .. }) =>
        {
            Some(())
        }
        _ => None,
    });
    let ast_stmt_kinds = take_ast
        .body
        .statements
        .iter()
        .map(|statement| match statement {
            rusty_valkyrie::frontend::ast::Statement::Let(_) => "let".to_string(),
            rusty_valkyrie::frontend::ast::Statement::Expression(expr) => match &expr.expr {
                rusty_valkyrie::frontend::ast::TermExpression::If { .. } => "if-expr".to_string(),
                rusty_valkyrie::frontend::ast::TermExpression::Return(_) => "return-expr".to_string(),
                other => format!("expr({:?})", std::mem::discriminant(other)),
            },
            other => format!("other({:?})", std::mem::discriminant(other)),
        })
        .collect::<Vec<_>>();
    assert!(if_stmt.is_some(), "parser must preserve if statement before option_none turbofish call, got {ast_stmt_kinds:?}");
    let hir = rusty_valkyrie::ValkyrieCompiler::default().compile_source(source).expect("option_none hir compile");
    let take = hir.functions.iter().find(|function| function.name.as_str() == "take").expect("take");
    let mut call_debug = Vec::new();
    fn walk_calls(expr: &HirExpr, out: &mut Vec<String>) {
        match &expr.kind {
            HirExprKind::Call { callee, resolved, .. } => {
                let name = extract_callable_name(callee).map(|id| id.to_string()).unwrap_or_else(|| format!("{:?}", callee.kind));
                let resolved = resolved
                    .as_ref()
                    .map(|call| {
                        format!("{} domain={:?} sum={}/{}", call.symbol, call.domain, call.sum_owner.is_some(), call.sum_variant.is_some())
                    })
                    .unwrap_or_else(|| "unresolved".to_string());
                out.push(format!("{name} => {resolved}"));
                if let HirExprKind::GenericApply { callee, .. } = &callee.kind {
                    walk_calls(callee, out);
                }
            }
            HirExprKind::Return(Some(inner)) | HirExprKind::Yield(Some(inner)) => walk_calls(inner, out),
            HirExprKind::If { condition, then_branch, else_branch, .. } => {
                walk_calls(condition, out);
                if let Some(expr) = &then_branch.expr {
                    walk_calls(expr, out);
                }
                for statement in &then_branch.statements {
                    if let HirStatementKind::Expr(expr) = &statement.kind {
                        walk_calls(expr, out);
                    }
                }
                if let Some(else_branch) = else_branch {
                    if let Some(expr) = &else_branch.expr {
                        walk_calls(expr, out);
                    }
                    for statement in &else_branch.statements {
                        if let HirStatementKind::Expr(expr) = &statement.kind {
                            walk_calls(expr, out);
                        }
                    }
                }
            }
            HirExprKind::Block(block) => {
                if let Some(expr) = &block.expr {
                    walk_calls(expr, out);
                }
                for statement in &block.statements {
                    if let HirStatementKind::Let { initializer: Some(initializer), .. } = &statement.kind {
                        walk_calls(initializer, out);
                    }
                    if let HirStatementKind::Expr(expr) = &statement.kind {
                        walk_calls(expr, out);
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(expr) = &take.body.expr {
        walk_calls(expr, &mut call_debug);
    }
    for statement in &take.body.statements {
        if let HirStatementKind::Expr(expr) = &statement.kind {
            walk_calls(expr, &mut call_debug);
        }
        if let HirStatementKind::Let { initializer: Some(initializer), .. } = &statement.kind {
            walk_calls(initializer, &mut call_debug);
        }
    }
    let if_shape = take.body.statements.first().map(|statement| format!("{:?}", statement.kind));
    let option_none_call = call_debug.iter().find(|line| line.starts_with("option_none"));
    assert!(
        option_none_call.is_some(),
        "expected option_none call in take body, got calls={call_debug:?} stmts={} tail={} first_stmt={if_shape:?}",
        take.body.statements.len(),
        take.body.expr.is_some()
    );
    let line = option_none_call.expect("option_none line");
    assert!(!line.contains("unresolved"), "option_none must resolve: {line}");
    assert!(line.contains("domain=Function"), "option_none must stay a normal function call: {line}");
    assert!(!line.contains("sum=true/true"), "option_none must not carry sum constructor facts: {line}");
}

#[test]
fn turbofish_generic_call_substitutes_return_type_and_keeps_callable_instance() {
    let hir = rusty_valkyrie::ValkyrieCompiler::default()
        .compile_source(
            r#"
namespace test;
unite Option<T> {
    Some { value: T },
    None,
}
micro option_none<T>() -> Option<T> {
    let empty: Option<T> = None
    return empty
}
micro pick(flag: bool) -> Option<i64> {
    if flag {
        return option_none::<i64>()
    }
    let value: i64 = 1
    return Some(value)
}
"#,
        )
        .expect("turbofish generic call compile");
    let pick = hir.functions.iter().find(|function| function.name.as_str() == "pick").expect("pick function");
    let mut resolved_call = None;
    fn walk_for_option_none_call(expr: &HirExpr, resolved_call: &mut Option<HirResolvedCall>) {
        match &expr.kind {
            HirExprKind::Call { callee, resolved, .. } if extract_callable_name(callee).is_some_and(|name| name.as_str() == "option_none") => {
                *resolved_call = resolved.clone();
            }
            HirExprKind::Return(Some(inner)) | HirExprKind::Yield(Some(inner)) => walk_for_option_none_call(inner, resolved_call),
            HirExprKind::If { condition, then_branch, else_branch, .. } => {
                walk_for_option_none_call(condition, resolved_call);
                if let Some(expr) = &then_branch.expr {
                    walk_for_option_none_call(expr, resolved_call);
                }
                for statement in &then_branch.statements {
                    if let HirStatementKind::Expr(expr) = &statement.kind {
                        walk_for_option_none_call(expr, resolved_call);
                    }
                }
                if let Some(else_branch) = else_branch {
                    if let Some(expr) = &else_branch.expr {
                        walk_for_option_none_call(expr, resolved_call);
                    }
                    for statement in &else_branch.statements {
                        if let HirStatementKind::Expr(expr) = &statement.kind {
                            walk_for_option_none_call(expr, resolved_call);
                        }
                    }
                }
            }
            HirExprKind::Block(block) => {
                if let Some(expr) = &block.expr {
                    walk_for_option_none_call(expr, resolved_call);
                }
                for statement in &block.statements {
                    if let HirStatementKind::Expr(expr) = &statement.kind {
                        walk_for_option_none_call(expr, resolved_call);
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(expr) = &pick.body.expr {
        walk_for_option_none_call(expr, &mut resolved_call);
    }
    for statement in &pick.body.statements {
        if let HirStatementKind::Expr(expr) = &statement.kind {
            walk_for_option_none_call(expr, &mut resolved_call);
        }
    }
    let resolved = resolved_call.expect("pick must contain resolved option_none turbofish call");
    assert_eq!(resolved.domain, HirCallableDomain::Function);
    assert!(resolved.instance.is_some(), "option_none turbofish call must keep callable instance");
    assert!(resolved.declaration.is_some(), "option_none turbofish call must keep declaration identity");
    assert!(resolved.sum_owner.is_none(), "option_none must not be rewritten into None constructor");
    assert!(resolved.sum_variant.is_none(), "option_none must not be rewritten into None constructor");
    assert_eq!(
        resolved.return_type,
        ValkyrieType::Nominal {
            declaration: hir.enums[0].declaration.expect("sum 声明身份"),
            arguments: vec![ValkyrieType::Named(Identifier::new("i64"))],
        }
    );
}

#[test]
fn resolves_array_subscript_on_generic_heap_array_param() {
    let source = r#"
micro get<T>(array: [T], ordinal: i64): T {
    return array::[ordinal]
}
"#;
    rusty_valkyrie::ValkyrieCompiler::default().compile_source(source).expect("generic heap array subscript");
}

fn test_sum_nominal(declaration_index: u32, arguments: Vec<ValkyrieType>) -> ValkyrieType {
    ValkyrieType::Nominal { declaration: nyar_types::ItemId::from_index(declaration_index).expect("test ItemId"), arguments }
}

#[test]
fn nullary_none_resolution_uses_return_type_owner_context() {
    let option_none = nullary_variant_constructor("Option", "None", 1);
    let other_none = nullary_variant_constructor("Other", "None", 99);
    let contextual = test_sum_nominal(1, vec![ValkyrieType::Integer32 { signed: true }]);
    let resolved = try_resolve_nullary_constructor(
        &Identifier::new("None"),
        Some(&contextual),
        &[option_none, other_none],
        &TypeRelationContext::from_module(&HirModule::default()),
        &BTreeMap::new(),
        &AggregateFieldIndex::default(),
        &BTreeSet::new(),
    )
    .expect("return Nominal Option<i32> should close Option::None");
    assert_eq!(resolved.sum_owner, nyar_types::ItemId::from_index(1));
    assert!(resolved.sum_variant.is_some());
    assert_eq!(resolved.domain, HirCallableDomain::Constructor);
}

#[test]
fn enrich_sum_variant_operation_facts_attaches_option_some_for_unwrap() {
    let option_some = nullary_variant_constructor("Option", "Some", 1);
    let option_ty = test_sum_nominal(1, vec![ValkyrieType::Integer32 { signed: true }]);
    let mut call = HirResolvedCall {
        declaration: None,
        instance: None,
        intrinsic: None,
        sum_owner: None,
        sum_variant: None,
        symbol: NamePath::new(vec![Identifier::new("unwrap")]),
        domain: HirCallableDomain::Function,
        return_type: ValkyrieType::Integer32 { signed: true },
        parameter_types: vec![option_ty.clone()],
        has_receiver: true,
        extractor_payload_type: None,
    };
    enrich_sum_variant_operation_facts(&mut call, Some(&option_ty), &[option_some]);
    assert!(call.sum_owner.is_some());
    assert!(call.sum_variant.is_some());
    assert_eq!(call.domain, HirCallableDomain::Extractor);
    assert_eq!(call.extractor_payload_type, Some(ValkyrieType::Integer32 { signed: true }));
}

fn nullary_variant_constructor(owner: &str, variant: &str, sum_owner_index: u32) -> OverloadCandidate {
    let sum_owner = nyar_types::ItemId::from_index(sum_owner_index).expect("test sum_owner");
    let mut candidate = OverloadCandidate::new_method(
        Identifier::new(owner),
        NamePath::new(vec![Identifier::new(variant)]),
        OverloadDomain::Constructor,
        Vec::new(),
        test_sum_nominal(sum_owner_index, vec![test_generic("T")]),
        OverloadMatchKind::Row,
    )
    .with_generic_binder(test_binder(900, 0));
    candidate.sum_owner = Some(sum_owner);
    candidate.sum_variant = nyar_types::ItemId::from_index(sum_owner_index + 1);
    candidate
}

#[test]
fn constructor_contract_does_not_substitute_nominal_parameter_as_generic() {
    let candidate = generic_constructor("Envelope", vec![ValkyrieType::Named(Identifier::new("DeclaredPayload"))]);
    assert!(resolve_constructor_contract(&[candidate], &[Some(ValkyrieType::Utf8)]).is_none());
}

#[test]
fn constructor_contract_from_source_preserves_generic_sum_signature() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 5210 })
        .compile_source(
            r#"
unite Envelope<T> {
    Present { value: T },
    Absent,
}
micro wrap(value: utf8) -> Envelope<utf8> {
    let wrapped = Present { value: value }
    return wrapped
}
"#,
        )
        .expect("当前源码中的构造声明应完成实例化");
    let function = hir.functions.iter().find(|function| function.name.as_str() == "wrap").expect("wrap");
    let HirStatementKind::Let { initializer: Some(initializer), .. } = &function.body.statements[0].kind
    else {
        panic!("缺少构造表达式");
    };
    let HirExprKind::Construct { resolved: Some(resolved), .. } = &initializer.kind
    else {
        panic!("构造身份未解析：{initializer:?}");
    };
    assert_eq!(resolved.symbol, NamePath::new(vec![Identifier::new("Envelope"), Identifier::new("Present")]));
    assert_eq!(resolved.parameter_types, vec![ValkyrieType::Utf8]);
    let envelope = hir.enums.iter().find(|item| item.name.as_str() == "Envelope").expect("Envelope");
    let declaration = envelope.declaration.expect("Envelope declaration");
    assert_eq!(resolved.return_type, ValkyrieType::Nominal { declaration, arguments: vec![ValkyrieType::Utf8] });
}

#[test]
fn ambiguous_bare_new_does_not_resolve_via_simple_name_fallback() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 5201 });
    let hir = compiler.compile_source(
        r#"
structure Alpha { value: i64 }

structure Beta { value: i64 }

imply Alpha {
    micro new(value: i64): Self {
        return Alpha { value: value }
    }
}

imply Beta {
    micro new(value: i64): Self {
        return Beta { value: value }
    }
}

micro main() {
    let x = new(1)
}
"#,
    );

    match hir {
        Ok(hir) => {
            let main = hir.functions.iter().find(|function| function.name.as_str() == "main").expect("main");
            let mut saw_bare_new = false;
            for statement in &main.body.statements {
                visit_calls_looking_for_bare_new(&statement.kind, &mut saw_bare_new);
            }
            assert!(saw_bare_new, "expected bare `new` call in main");
        }
        Err(error) => {
            let text = error.to_string();
            assert!(
                text.contains("new") || text.contains("SMIR003") || text.contains("unresolved") || text.contains("ambiguous"),
                "ambiguous bare `new` must fail closed without inventing a binding, got {text}"
            );
        }
    }
}

fn visit_calls_looking_for_bare_new(kind: &HirStatementKind, saw_bare_new: &mut bool) {
    let expression = match kind {
        HirStatementKind::Let { initializer: Some(value), .. } | HirStatementKind::Expr(value) => value.as_ref(),
        _ => return,
    };
    if let HirExprKind::Call { callee, resolved, .. } = &expression.kind {
        if extract_callable_name(callee).is_some_and(|name| name.as_str() == "new") {
            *saw_bare_new = true;
            assert!(resolved.is_none(), "ambiguous bare `new` must stay unresolved, got {resolved:?}");
        }
    }
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn qualified_type_static_call_does_not_fall_back_to_unrelated_new() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 5202 });
    let result = compiler.compile_source(
        r#"
structure HashMap<K, V> {
    capacity: i64
}

structure TuiRuntime {
    capacity: i64
}

imply HashMap<K, V> {
    micro new(capacity: i64): Self {
        return HashMap { capacity: capacity }
    }
}

imply TuiRuntime {
    micro new(capacity: i64): Self {
        return TuiRuntime { capacity: capacity }
    }
}

micro main() {
    let map: HashMap<i64, i64> = HashMap::new(0)
}
"#,
    );
    let hir = result.expect("qualified static call must resolve from its declared signature");
    let main = hir.functions.iter().find(|function| function.name.as_str() == "main").expect("main");
    let resolved = main
        .body
        .statements
        .iter()
        .find_map(|statement| match &statement.kind {
            HirStatementKind::Let { initializer: Some(value), .. } => match &value.kind {
                HirExprKind::Call { resolved: Some(resolved), .. } => Some(resolved),
                _ => None,
            },
            _ => None,
        })
        .expect("HashMap::new must carry a resolved call contract");
    assert_eq!(resolved.symbol.to_string(), "HashMap.new");
    assert_eq!(resolved.parameter_types, vec![ValkyrieType::Integer64 { signed: true }]);
}

#[test]
fn receiver_presence_is_derived_from_the_declared_self_parameter() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 5203 });
    let hir = compiler
        .compile_source(
            r#"
structure Counter { value: i64 }

imply Counter {
    micro read(self): i64 { self.value }
    micro make(value: i64): i64 { value }
}

micro main(counter: Counter) {
    let current = counter.read()
    let created = Counter.make(1)
}
"#,
        )
        .expect("declared instance and static methods must resolve");
    let main = hir.functions.iter().find(|function| function.name.as_str() == "main").expect("main");
    let resolved = main
        .body
        .statements
        .iter()
        .filter_map(|statement| match &statement.kind {
            HirStatementKind::Let { initializer: Some(value), .. } => match &value.kind {
                HirExprKind::Call { resolved: Some(resolved), .. } => Some(resolved),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].symbol.to_string(), "Counter.read");
    assert_eq!(
        resolved[0].parameter_types,
        vec![ValkyrieType::Nominal { declaration: hir.structs[0].constructor_declaration.expect("receiver 声明身份"), arguments: vec![] }]
    );
    assert!(resolved[0].has_receiver);
    assert_eq!(resolved[1].symbol.to_string(), "Counter.make");
    assert_eq!(resolved[1].parameter_types, vec![ValkyrieType::Named(Identifier::new("i64"))]);
    assert!(!resolved[1].has_receiver);
}

#[test]
fn instance_syntax_cannot_supply_a_missing_self_parameter() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 5204 });
    let error = compiler
        .compile_source(
            r#"
structure Counter { }

imply Counter {
    micro make(value: i64): i64 { value }
}

micro main(counter: Counter) {
    counter.make(1)
}
"#,
        )
        .expect_err("a static method must not gain a receiver from dot syntax");
    let message = error.to_string();
    assert!(message.contains("SMIR003") || message.contains("unresolved"), "unexpected failure: {message}");
}
