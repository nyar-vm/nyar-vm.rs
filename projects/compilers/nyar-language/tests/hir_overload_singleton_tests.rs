use nyar_language::{
    Identifier, NamePath, ValkyrieCompiler,
    types::SourceID,
    valkyrie::{
        HirError,
        hir::{
            overload::{OverloadCandidate, OverloadDomain, OverloadMatchKind},
            type_relation::{ParameterMatchResult, TypeRelationContext},
        },
        mir::AggregateFieldIndex,
        types::hir::{
            HirCallArgument, HirCallableDomain, HirExpr, HirExprKind, HirFunction, HirIdentifier, HirModule, HirStatementKind,
            types::ValkyrieType,
        },
    },
};
use nyar_types::IntrinsicId;
use std::collections::{BTreeMap, BTreeSet};

use nyar_language::valkyrie::hir::overload::{collect_module_candidates, match_call_candidate};

fn test_binder(owner: u32, ordinal: u32) -> nyar_language::valkyrie::types::hir::GenericBinderId {
    nyar_language::valkyrie::types::hir::GenericBinderId { owner: nyar_types::ItemId::from_index(owner).unwrap(), ordinal }
}

fn test_generic(name: &str) -> ValkyrieType {
    ValkyrieType::Generic(nyar_language::valkyrie::types::hir::GenericType {
        binder: Some(test_binder(900, 0)),
        name: Identifier::new(name),
        kind: nyar_language::valkyrie::types::hir::HirKind::Type,
        bounds: vec![],
    })
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn imply_bool_infix_not_eq_resolves_eq_operator_call() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4211 });
    compiler
        .compile_source(
            r#"
[primitive("core::primitive::bool")]
structure bool { }

imply bool {
    infix `==`(self, rhs: Self): bool {
        if self { return rhs }
        return !rhs
    }
    infix `!=`(self, rhs: Self): bool {
        !(self == rhs)
    }
}
"#,
        )
        .expect("bool operator imply must resolve");
}

#[test]
fn primitive_intrinsic_declaration_resolves_without_helper_name_registry() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4210 });
    let hir = compiler
        .compile_source(
            r#"
[primitive("core::primitive::f64")]
structure f64 { }

imply f64 {
    infix `+`(self, rhs: Self): Self {
        __f64_add(self, rhs)
    }
}

[intrinsic("f64.add")]
private micro __f64_add(lhs: f64, rhs: f64): f64 { }
"#,
        )
        .expect("primitive intrinsic declaration must resolve");
    let method = &hir.impls[0].methods[0];
    let HirExprKind::Call { resolved: Some(ref resolved), .. } = method.body.expr.as_ref().expect("operator body expression").kind
    else {
        panic!("expected resolved intrinsic call: {:?}", method.body);
    };
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn resolves_singleton_instance_method_from_singleton_owner() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4201 });
    let hir = compiler
        .compile_source(
            r#"
singleton Counter {
    mut total: i64 = 0

    micro increment(mut self) -> i64 {
        self.total += 1
        self.total
    }
}

micro main() -> i64 {
    return Counter.increment();
}
"#,
        )
        .unwrap();

    let main = hir.functions.iter().find(|function| function.name.as_str() == "main").expect("main");
    let HirStatementKind::Expr(statement) = &main.body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
    else {
        panic!("expected resolved singleton call");
    };

    assert_eq!(resolved.domain, HirCallableDomain::Function);
    assert_eq!(resolved.symbol.to_string(), "Counter.increment");
    assert_eq!(resolved.return_type, ValkyrieType::Integer64 { signed: true });
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn resolves_singleton_self_method_call_inside_method_body() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4203 });
    let hir = compiler
        .compile_source(
            r#"
singleton Counter {
    mut total: i64 = 0

    micro increment(mut self) -> i64 {
        self.total += 1
        self.total
    }

    micro tick(mut self) -> i64 {
        return self.increment();
    }
}
"#,
        )
        .unwrap();

    let tick = hir.singletons[0].methods.iter().find(|method| method.name.as_str() == "tick").expect("tick");
    let HirStatementKind::Expr(statement) = &tick.body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Call { args, resolved: Some(resolved), .. } = &expression.kind
    else {
        panic!("expected resolved singleton self call");
    };

    assert_eq!(args.len(), 1);
    assert!(matches!(args[0].value.kind, HirExprKind::Variable(ref ident) if ident.name.as_str() == "self"));
    assert_eq!(resolved.domain, HirCallableDomain::Function);
    assert_eq!(resolved.symbol.to_string(), "Counter.increment");
    assert_eq!(resolved.return_type, ValkyrieType::Integer64 { signed: true });
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn resolves_singleton_self_field_access_as_overload_argument() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4204 });
    let hir = compiler
        .compile_source(
            r#"
micro choose(value: bool) -> bool {
    return value;
}

micro choose(value: i64) -> i64 {
    return value;
}

singleton Counter {
    mut total: i64 = 0

    micro current(self) -> i64 {
        return choose(self.total);
    }
}
"#,
        )
        .unwrap();

    let current = hir.singletons[0].methods.iter().find(|method| method.name.as_str() == "current").expect("current");
    let HirStatementKind::Expr(statement) = &current.body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Call { args, resolved: Some(resolved), .. } = &expression.kind
    else {
        panic!("expected resolved overload call");
    };

    assert_eq!(args.len(), 1);
    assert!(matches!(args[0].value.kind, HirExprKind::FieldAccess { ref field, .. } if field.as_str() == "total"));
    assert_eq!(resolved.domain, HirCallableDomain::Function);
    assert_eq!(resolved.symbol.to_string(), "main::choose");
    assert_eq!(resolved.return_type, ValkyrieType::Integer64 { signed: true });
}

#[test]
fn resolves_same_module_helper_with_stable_qualified_symbol() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4207 });
    let hir = compiler
        .compile_source(
            r#"
micro main(): i64 {
    let value: i64 = answer()
    return value
}

micro answer(): i64 {
    return 42
}
"#,
        )
        .unwrap();

    let main = hir.functions.iter().find(|function| function.name.as_str() == "main").expect("main");
    let HirStatementKind::Let { initializer: Some(initializer), .. } = &main.body.statements[0].kind
    else {
        panic!("expected let initializer");
    };
    let HirExprKind::Call { resolved: Some(resolved), .. } = &initializer.kind
    else {
        panic!("expected resolved helper call");
    };
    assert_eq!(resolved.domain, HirCallableDomain::Function);
    assert_eq!(resolved.symbol.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>(), vec!["main", "answer"]);
}

#[test]
fn qualified_namespace_call_uses_canonical_identity() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4210 });
    let hir = compiler
        .compile_source(
            r#"
namespace library.console;
micro emit(flag: bool): unit {}
namespace library.client;
micro invoke(): unit { library.console.emit(true) }
"#,
        )
        .expect("qualified source call");
    fn find(module: &HirModule) -> Option<&HirFunction> {
        module.functions.iter().find(|function| function.name.as_str() == "invoke").or_else(|| module.submodules.iter().find_map(find))
    }
    let function = find(&hir).expect("invoke");
    let expression = function.body.expr.as_ref().expect("call expression");
    let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
    else {
        panic!("qualified call must resolve");
    };
    assert_eq!(resolved.symbol.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>(), vec!["library", "console", "emit"]);
}

#[test]
fn submodule_imply_body_resolves_qualified_call() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4211 });
    let hir = compiler
        .compile_source(
            r#"
namespace library.console;
micro emit(flag: bool): unit {}
namespace library.client;
class Client {}
imply Client {
    micro invoke(): unit { library.console.emit(true) }
}
"#,
        )
        .expect("qualified imply source call");
    fn find(module: &HirModule) -> Option<&HirFunction> {
        module
            .impls
            .iter()
            .flat_map(|item| &item.methods)
            .find(|function| function.name.as_str() == "invoke")
            .or_else(|| module.submodules.iter().find_map(find))
    }
    let function = find(&hir).expect("invoke");
    let expression = function.body.expr.as_ref().expect("call expression");
    let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
    else {
        panic!("qualified imply call must resolve");
    };
    assert_eq!(resolved.symbol.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>(), vec!["library", "console", "emit"]);
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn method_generic_parameters_survive_hir_lowering() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4212 });
    let hir = compiler
        .compile_source(
            r#"
class Collector {
    micro identity<T>(value: T) -> T where T: Iterator<Item = i32> { return value; }
}
"#,
        )
        .expect("generic method source");
    let method = &hir.structs[0].methods[0];
    assert_eq!(method.generics.len(), 1);
    assert_eq!(method.generics[0].name.as_str(), "T");
    assert_eq!(method.params.len(), 1);
    assert_eq!(method.params[0].ty, method.return_type);
    assert_eq!(method.where_constraints.len(), 1);
    let constraint = &method.where_constraints[0];
    assert_eq!(constraint.bounds.len(), 1);
    let bound = &constraint.bounds[0];
    assert_eq!(bound.associated_types.len(), 1);
    assert_eq!(bound.trait_path.to_string(), "Iterator");
    assert_eq!(bound.associated_types[0].name.as_str(), "Item");
    assert_eq!(bound.associated_types[0].ty, ValkyrieType::Integer32 { signed: true });
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn where_bounds_preserve_positional_arguments_and_equation_ownership() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4214 });
    let hir = compiler
        .compile_source(
            r#"
micro identity<T>(value: T) -> T
    where T: Mapper<i32, bool, Item = i32> + Mapper<bool, i32, Item = bool>
{ return value; }
"#,
        )
        .expect("structured generic bounds");
    let bounds = &hir.functions[0].where_constraints[0].bounds;
    assert_eq!(bounds.len(), 2);
    assert_eq!(bounds[0].trait_path, bounds[1].trait_path);
    let integer = ValkyrieType::Integer32 { signed: true };
    assert_eq!(bounds[0].type_arguments, vec![integer.clone(), ValkyrieType::Boolean]);
    assert_eq!(bounds[1].type_arguments, vec![ValkyrieType::Boolean, integer.clone()]);
    assert_eq!(bounds[0].associated_types[0].ty, integer);
    assert_eq!(bounds[1].associated_types[0].ty, ValkyrieType::Boolean);
}

#[test]
fn unrelated_generic_declaration_does_not_make_nominal_type_a_wildcard() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4213 });
    let hir = compiler
        .compile_source(
            r#"
class T {}
micro identity<T>(value: T) -> T { return value; }
"#,
        )
        .expect("nominal and scoped generic declarations");
    let relations = TypeRelationContext::from_module(&hir);
    assert!(matches!(relations.match_parameter(&ValkyrieType::Boolean, &test_generic("T")), ParameterMatchResult::NoMatch { .. }));
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn array_of_utf8_length_resolves_to_array_not_utf8_text() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4202 });
    let hir = compiler
        .compile_source(
            r#"
namespace test;

type utf8 = Utf8Text

class Utf8Text {}

imply Utf8Text {
    micro length(self) -> i32 {
        return 0
    }
}

class Array<T> {}

imply Array<T> {
    micro length(self): usize {
        return 0
    }
}

micro dispatch(args: [utf8]) -> usize {
    return args.length()
}
"#,
        )
        .expect("compile");
    let dispatch = hir.functions.iter().find(|function| function.name.as_str() == "dispatch").expect("dispatch");
    let HirStatementKind::Expr(statement) = &dispatch.body.statements[0].kind
    else {
        panic!("expected return statement, body={:?}", dispatch.body);
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression, got {statement:?}");
    };
    let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
    else {
        panic!("expected resolved length call, got {expression:?}");
    };
    let symbol = resolved.symbol.to_string();
    assert!(symbol.contains("Array"), "expected Array.length for [utf8].length(), got {symbol}");
    assert!(!symbol.contains("Utf8Text"), "[utf8].length() must not bind Utf8Text.length, got {symbol}");
}

#[test]
fn generic_function_can_construct_its_own_nominal_variant() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4206 });
    let hir = compiler
        .compile_source(
            r#"
namespace test;

unite Envelope<T> {
    Present { value: T },
    Absent,
}

micro wrap<T>(value: T) -> Envelope<T> {
    let wrapped: Envelope<T> = Present(value)
    return wrapped
}
"#,
        )
        .expect("generic variant construction must resolve");
    let function = hir.functions.iter().find(|function| function.name.as_str() == "wrap").expect("wrap");
    let HirStatementKind::Let { initializer: Some(initializer), .. } = &function.body.statements[0].kind
    else {
        panic!("expected generic construction initializer")
    };
    let HirExprKind::Call { resolved: Some(resolved), .. } = &initializer.kind
    else {
        panic!("expected resolved generic variant construction: {initializer:?}")
    };
    assert_eq!(resolved.domain, HirCallableDomain::Constructor);
}

#[test]
fn utf8_slice_without_adaptor_is_rejected_before_mir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4203 });
    let error = compiler
        .compile_source(
            r#"
namespace test;

micro slice_probe(text: utf8, start: i32, count: i32) -> utf8 {
    return text.slice(start, count)
}
"#,
        )
        .expect_err("missing adaptor contract must fail before MIR");
    assert!(error.to_string().contains("SMIR003"), "{error}");
    assert!(error.to_string().contains("slice_probe"), "{error}");
}

#[test]
fn utf8_loop_without_adaptor_is_rejected_before_mir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4204 });
    let error = compiler
        .compile_source(
            r#"
namespace test;

micro slice_loop_probe(text: utf8, op: utf8) -> i32 {
    let n: i32 = text.length()
    let count: i32 = op.length()
    let mut i: i32 = 0
    while i <= n - count {
        let ch: utf8 = text.slice(i, 1)
        if ch == "(" {
            i = i + 1
            continue
        }
        if text.slice(i, count) == op {
            return i
        }
        i = i + 1
    }
    return 0
}
"#,
        )
        .expect_err("loop control flow must not bypass missing adaptor contracts");
    assert!(error.to_string().contains("SMIR003"), "{error}");
    assert!(error.to_string().contains("slice_loop_probe"), "{error}");
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn boolean_not_uses_structured_intrinsic() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4205 });
    let hir = compiler
        .compile_source(
            r#"
namespace test;

micro negate_flag(flag: bool) -> bool {
    return !flag
}
"#,
        )
        .expect("boolean not must resolve through the structured contract");
    let function = hir.functions.iter().find(|function| function.name.as_str() == "negate_flag").expect("negate_flag");
    let HirStatementKind::Expr(statement) = &function.body.statements[0].kind
    else {
        panic!("expected return statement")
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression")
    };
    let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
    else {
        panic!("expected resolved boolean not: {expression:?}")
    };
    assert_eq!(resolved.parameter_types, vec![ValkyrieType::Boolean]);
}

#[test]
fn array_length_value_access_resolves_through_builtin_contract() {
    let compiler = ValkyrieCompiler::default();
    let hir = compiler
        .compile_source(
            r#"
micro len(items: [i32]) -> usize {
    let count: usize = items.length
    return count
}
"#,
        )
        .expect("compile");
    let function = hir.functions.iter().find(|item| item.name.as_str() == "len").expect("len");
    let HirStatementKind::Let { initializer: Some(initializer), .. } = &function.body.statements[0].kind
    else {
        panic!("expected let with initializer, got {:?}", function.body.statements[0].kind);
    };
    // 值位置成员访问没有独立语义：`[T].length` 是 `builtin.array.length` 的零实参调用。
    // 接收者作首个实参，语言身份由已解析的 builtin 符号承载，MIR 不再按字段名 `length` 猜长度。
    let HirExprKind::Call { callee, args, resolved } = &initializer.kind
    else {
        panic!("expected resolved length call, got {initializer:?}");
    };
    let HirExprKind::Path(path) = &callee.kind
    else {
        panic!("expected bare member callee, got {callee:?}");
    };
    assert_eq!(path.parts().len(), 1, "member callee must be the bare selection name: {path}");
    assert_eq!(args.len(), 1, "receiver must be the first argument: {args:?}");
    let resolved = resolved.as_ref().expect("array length must resolve through the declared builtin candidate");
    let symbol = resolved.symbol.parts().iter().map(ToString::to_string).collect::<Vec<_>>();
    assert_eq!(symbol, vec!["builtin", "array", "length"], "resolved identity must be the builtin candidate symbol");
    assert!(matches!(&resolved.return_type, ValkyrieType::Named(name) if name.as_str() == "usize"));
}

#[test]
fn array_len_intrinsic_contract_specializes_generic_array() {
    let span = nyar_language::types::SourceSpan::new(nyar_language::types::SourceID::default(), 0, 0);
    let candidate = OverloadCandidate::new(
        NamePath::new(vec![Identifier::new("builtin"), Identifier::new("array"), Identifier::new("length")]),
        OverloadDomain::Function,
        vec![ValkyrieType::Array(Box::new(test_generic("T")))],
        ValkyrieType::Named(Identifier::new("usize")),
        OverloadMatchKind::NominalExact,
    )
    .with_generic_binder(test_binder(900, 0));
    let items_ty = ValkyrieType::Array(Box::new(ValkyrieType::Integer32 { signed: true }));
    let locals = BTreeMap::from([("items".to_string(), items_ty.clone())]);
    let args = vec![HirCallArgument::positional(HirExpr {
        kind: HirExprKind::Variable(HirIdentifier { name: Identifier::new("items"), shadow_index: 0, span: span.clone() }),
        span: span.clone(),
    })];
    let matched = match_call_candidate(
        &candidate,
        &args,
        &[],
        &TypeRelationContext::from_module(&HirModule::default()),
        &locals,
        &AggregateFieldIndex::default(),
        &BTreeSet::new(),
    )
    .expect("ArrayLen intrinsic must match array receiver");
    assert_eq!(matched.signature.params, vec![items_ty]);
    assert_eq!(matched.signature.return_type, ValkyrieType::Named(Identifier::new("usize")));
}

#[test]
fn array_len_intrinsic_contract_rejects_non_array_receiver() {
    let span = nyar_language::types::SourceSpan::new(nyar_language::types::SourceID::default(), 0, 0);
    let candidate = OverloadCandidate::new(
        NamePath::new(vec![Identifier::new("builtin"), Identifier::new("array"), Identifier::new("length")]),
        OverloadDomain::Function,
        vec![ValkyrieType::Array(Box::new(test_generic("T")))],
        ValkyrieType::Named(Identifier::new("usize")),
        OverloadMatchKind::NominalExact,
    )
    .with_generic_binder(test_binder(900, 0));
    let locals = BTreeMap::from([("text".to_string(), ValkyrieType::Utf8)]);
    let args = vec![HirCallArgument::positional(HirExpr {
        kind: HirExprKind::Variable(HirIdentifier { name: Identifier::new("text"), shadow_index: 0, span: span.clone() }),
        span: span.clone(),
    })];
    assert!(
        match_call_candidate(
            &candidate,
            &args,
            &[],
            &TypeRelationContext::from_module(&HirModule::default()),
            &locals,
            &AggregateFieldIndex::default(),
            &BTreeSet::new(),
        )
        .is_none(),
        "ArrayLen must not match non-array receivers"
    );
}

#[test]
fn nullable_intrinsic_contracts_specialize_nullable_operands() {
    let span = nyar_language::types::SourceSpan::new(nyar_language::types::SourceID::default(), 0, 0);
    let payload = ValkyrieType::Integer32 { signed: true };
    let nullable = ValkyrieType::Nullable(Box::new(payload.clone()));
    let locals = BTreeMap::from([("value".to_string(), nullable.clone())]);
    let args = vec![HirCallArgument::positional(HirExpr {
        kind: HirExprKind::Variable(HirIdentifier { name: Identifier::new("value"), shadow_index: 0, span: span.clone() }),
        span: span.clone(),
    })];
    let relations = TypeRelationContext::from_module(&HirModule::default());

    let is_null = OverloadCandidate::new(
        NamePath::new(vec![Identifier::new("builtin"), Identifier::new("null"), Identifier::new("is_null")]),
        OverloadDomain::Function,
        vec![nullable.clone()],
        ValkyrieType::Boolean,
        OverloadMatchKind::NominalExact,
    );
    let matched_is_null = match_call_candidate(&is_null, &args, &[], &relations, &locals, &AggregateFieldIndex::default(), &BTreeSet::new())
        .expect("IsNull intrinsic must match nullable receiver");
    assert_eq!(matched_is_null.signature.return_type, ValkyrieType::Boolean);

    let unwrap_null = OverloadCandidate::new(
        NamePath::new(vec![Identifier::new("builtin"), Identifier::new("null"), Identifier::new("unwrap")]),
        OverloadDomain::Function,
        vec![nullable.clone()],
        payload.clone(),
        OverloadMatchKind::NominalExact,
    );
    let matched_unwrap = match_call_candidate(&unwrap_null, &args, &[], &relations, &locals, &AggregateFieldIndex::default(), &BTreeSet::new())
        .expect("UnwrapNull intrinsic must match nullable receiver");
    assert_eq!(matched_unwrap.signature.return_type, payload);

    let ref_deref = OverloadCandidate::new(
        NamePath::new(vec![Identifier::new("builtin"), Identifier::new("ref"), Identifier::new("deref")]),
        OverloadDomain::Function,
        vec![nullable.clone()],
        payload.clone(),
        OverloadMatchKind::NominalExact,
    );
    let matched_deref = match_call_candidate(&ref_deref, &args, &[], &relations, &locals, &AggregateFieldIndex::default(), &BTreeSet::new())
        .expect("RefDeref intrinsic must match nullable receiver");
    assert_eq!(matched_deref.signature.return_type, payload);
}

#[test]
fn nullable_intrinsic_contracts_reject_non_nullable_operands() {
    let span = nyar_language::types::SourceSpan::new(nyar_language::types::SourceID::default(), 0, 0);
    let locals = BTreeMap::from([("value".to_string(), ValkyrieType::Integer32 { signed: true })]);
    let args = vec![HirCallArgument::positional(HirExpr {
        kind: HirExprKind::Variable(HirIdentifier { name: Identifier::new("value"), shadow_index: 0, span: span.clone() }),
        span: span.clone(),
    })];
    let relations = TypeRelationContext::from_module(&HirModule::default());
    let is_null = OverloadCandidate::new(
        NamePath::new(vec![Identifier::new("builtin"), Identifier::new("null"), Identifier::new("is_null")]),
        OverloadDomain::Function,
        vec![ValkyrieType::Nullable(Box::new(ValkyrieType::Integer32 { signed: true }))],
        ValkyrieType::Boolean,
        OverloadMatchKind::NominalExact,
    );
    assert!(
        match_call_candidate(&is_null, &args, &[], &relations, &locals, &AggregateFieldIndex::default(), &BTreeSet::new()).is_none(),
        "IsNull must not match non-nullable receivers"
    );
    let unwrap_null = OverloadCandidate::new(
        NamePath::new(vec![Identifier::new("builtin"), Identifier::new("null"), Identifier::new("unwrap")]),
        OverloadDomain::Function,
        vec![ValkyrieType::Nullable(Box::new(ValkyrieType::Integer32 { signed: true }))],
        ValkyrieType::Integer32 { signed: true },
        OverloadMatchKind::NominalExact,
    );
    assert!(
        match_call_candidate(&unwrap_null, &args, &[], &relations, &locals, &AggregateFieldIndex::default(), &BTreeSet::new()).is_none(),
        "UnwrapNull must not match non-nullable receivers"
    );
}

#[test]
fn array_push_resolves_to_intrinsic_builtin_symbol() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4210 });
    let hir = compiler
        .compile_source(
            r#"
namespace test;

micro grow(text: utf8) -> [utf8] {
    let mut result: [utf8] = []
    push(result, text)
    return result
}
"#,
        )
        .expect("array push must compile");
    let function = hir.functions.iter().find(|function| function.name.as_str() == "grow").expect("grow");
    let HirStatementKind::Expr(statement) = &function.body.statements[1].kind
    else {
        panic!("expected push statement")
    };
    let HirExprKind::Call { resolved: Some(resolved), .. } = &statement.kind
    else {
        panic!("push must resolve via IntrinsicId builtin candidate: {statement:?}")
    };
    let parts = resolved.symbol.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>();
    assert_eq!(IntrinsicId::resolve_from_segments(&parts), Some(IntrinsicId::ArrayPush));
}

#[test]
fn array_push_resolves_inside_imply_with_declared_element_type() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4211 });
    let hir = compiler
        .compile_source(
            r#"
namespace test;

class Utf8Text {}

imply Utf8Text {
    micro split(self, separator: utf8) -> [utf8] {
        let mut result: [utf8] = []
        push(result, separator)
        push(result, "")
        return result
    }
}
"#,
        )
        .expect("imply array push must compile");
    let method = hir.impls.iter().flat_map(|item| item.methods.iter()).find(|method| method.name.as_str() == "split").expect("split");
    let mut push_calls = 0usize;
    for statement in &method.body.statements {
        fn walk(expr: &HirExpr, push_calls: &mut usize) {
            match &expr.kind {
                HirExprKind::Call { resolved, args, .. } => {
                    if let Some(resolved) = resolved {
                        let parts = resolved.symbol.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>();
                        if IntrinsicId::resolve_from_segments(&parts) == Some(IntrinsicId::ArrayPush) {
                            *push_calls += 1;
                        }
                    }
                    for arg in args {
                        walk(&arg.value, push_calls);
                    }
                }
                HirExprKind::If { then_branch, else_branch, condition, .. } => {
                    walk(condition, push_calls);
                    for statement in &then_branch.statements {
                        if let HirStatementKind::Expr(inner) = &statement.kind {
                            walk(inner, push_calls);
                        }
                    }
                    if let Some(else_branch) = else_branch {
                        for statement in &else_branch.statements {
                            if let HirStatementKind::Expr(inner) = &statement.kind {
                                walk(inner, push_calls);
                            }
                        }
                    }
                }
                HirExprKind::Return(Some(inner)) => walk(inner, push_calls),
                HirExprKind::Block(block) => {
                    if let Some(inner) = &block.expr {
                        walk(inner, push_calls);
                    }
                }
                _ => {}
            }
        }
        if let HirStatementKind::Expr(expr) = &statement.kind {
            walk(expr, &mut push_calls);
        }
    }
    assert_eq!(push_calls, 2, "both calls must bind the declared ArrayPush identity");
}

#[test]
fn debug_ascii_alnum_key_some_construct_resolution() {
    let source = r#"
namespace core::types;
unite Option<T> { Some { value: T }, None }
namespace core::text;
structure char { }
imply char {
    micro ascii_alnum_key(self) -> Option<i64> {
        if true { return Some { value: 1 as i64 } }
        return None
    }
}
"#;
    let root = nyar_language::valkyrie::frontend::parse_source(source).expect("parse");
    let hir =
        nyar_language::AstToHir::new(SourceID { version_id: 0 }).lower_root_with_semantic_exports_and_name(&root, &[], None).expect("lower");
    let option = hir.enums.iter().find(|item| item.name.as_str() == "Option").expect("Option");
    eprintln!("Option generics: {:?}", option.generics);
    eprintln!("Some variant: {:?}", option.variants.iter().find(|variant| variant.name.as_str() == "Some"));
    let candidates = collect_module_candidates(&hir);
    let some_candidates: Vec<_> = candidates
        .iter()
        .filter(|candidate| candidate.domain == OverloadDomain::Constructor)
        .filter(|candidate| candidate.symbol.parts().last().is_some_and(|name| name.as_str() == "Some"))
        .collect();
    eprintln!("Some constructor candidates: {}", some_candidates.len());
    for candidate in &some_candidates {
        eprintln!(
            "  symbol={:?} owner={:?} binders={} return={:?} params={:?} sum={:?}/{:?}",
            candidate.symbol,
            candidate.owner,
            candidate.generic_binders.len(),
            candidate.signature.return_type,
            candidate.signature.params,
            candidate.sum_owner,
            candidate.sum_variant
        );
    }
    let method = hir
        .impls
        .iter()
        .flat_map(|implementation| implementation.methods.iter())
        .find(|function| function.name.as_str() == "ascii_alnum_key")
        .expect("method");
    eprintln!("return_type: {:?}", method.return_type);
    let mut constructs = Vec::new();
    fn walk(expr: &HirExpr, out: &mut Vec<String>) {
        match &expr.kind {
            HirExprKind::Construct { path, resolved, .. } => {
                out.push(format!(
                    "{path} => {}",
                    resolved
                        .as_ref()
                        .map(|call| format!("domain={:?} sum={}/{}", call.domain, call.sum_owner.is_some(), call.sum_variant.is_some()))
                        .unwrap_or_else(|| "unresolved".to_string())
                ));
            }
            HirExprKind::Return(Some(inner)) => walk(inner, out),
            HirExprKind::If { condition, then_branch, else_branch, .. } => {
                walk(condition, out);
                if let Some(expr) = &then_branch.expr {
                    walk(expr, out);
                }
                for statement in &then_branch.statements {
                    if let HirStatementKind::Expr(expr) = &statement.kind {
                        walk(expr, out);
                    }
                }
                if let Some(else_branch) = else_branch {
                    if let Some(expr) = &else_branch.expr {
                        walk(expr, out);
                    }
                }
            }
            _ => {}
        }
    }
    for statement in &method.body.statements {
        if let HirStatementKind::Expr(expr) = &statement.kind {
            walk(expr, &mut constructs);
        }
    }
    if let Some(expr) = &method.body.expr {
        walk(expr, &mut constructs);
    }
    eprintln!("constructs: {constructs:?}");
    assert!(constructs.iter().all(|line| !line.contains("unresolved")), "all constructs must resolve: {constructs:?}");
}

/// 正例：浮点字面量绑定到浮点形参（`f32` 与 `f64`），并保持同一声明身份。
#[test]
fn float_literal_binds_to_float_formal_parameter() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4221 });
    compiler
        .compile_source(
            r#"
[primitive("core::primitive::f32")]
structure f32 { }

[primitive("core::primitive::f64")]
structure f64 { }

micro take_f64(x: f64) -> f64 {
    return x
}

micro take_f32(x: f32) -> f32 {
    return x
}

micro keep_f64() -> f64 {
    return take_f64(1.0)
}

micro keep_f32() -> f32 {
    return take_f32(1.0)
}
"#,
        )
        .expect("浮点字面量必须绑定到浮点形参");
}

/// 正例：整数字面量仍按数值材质绑定到整数形参。
#[test]
fn integer_literal_still_binds_to_integer_formal_parameter() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4222 });
    compiler
        .compile_source(
            r#"
[primitive("core::primitive::i32")]
structure i32 { }

micro take_i32(x: i32) -> i32 {
    return x
}

micro keep_i32() -> i32 {
    return take_i32(1)
}
"#,
        )
        .expect("整数字面量必须绑定到整数形参");
}

/// 反例：浮点字面量不得按“同属数值”静默塞入整数形参，必须返回结构化失败。
#[test]
fn float_literal_does_not_bind_to_integer_formal_parameter() {
    for primitive in ["i32", "usize"] {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4223 });
        let error = compiler
            .compile_source(&format!(
                r#"
[primitive("core::primitive::{primitive}")]
structure {primitive} {{ }}

micro take(x: {primitive}) -> {primitive} {{
    return x
}}

micro keep() -> {primitive} {{
    return take(1.0)
}}
"#
            ))
            .expect_err("浮点字面量不得绑定到整数形参");
        let rendered = format!("{error:?}");
        assert!(matches!(error, HirError::UnresolvedCall { .. }), "浮点字面量绑定整数形参必须在 overload 阶段结构化失败: {rendered}");
        assert!(rendered.contains("take"), "失败必须指出未解析的调用: {rendered}");
    }
}
