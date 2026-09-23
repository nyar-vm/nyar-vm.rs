use nyar_language::{
    ValkyrieCompiler,
    types::{
        Identifier, SourceID,
        hir::{HirCallableDomain, HirExprKind, HirExtractorPattern, HirPattern, HirStatementKind, ValkyrieType},
    },
};

#[test]
fn resolves_plain_call_into_hir_metadata() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4101 });
    let hir = compiler
        .compile_source(
            r#"
micro choose(value: i64) -> i64 {
    return value;
}

micro main(value: i64) -> i64 {
    return choose(value);
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[1].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
    else {
        panic!("expected resolved call");
    };

    assert_eq!(resolved.domain, HirCallableDomain::Function);
    assert_eq!(resolved.symbol.to_string(), "choose");
    assert_eq!(resolved.return_type, ValkyrieType::Integer64 { signed: true });
}

#[test]
fn keeps_plain_function_call_as_function_domain() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4104 });
    let hir = compiler
        .compile_source(
            r#"
micro ready() -> bool {
    return true;
}

micro main() -> bool {
    return ready();
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[1].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Call { callee, resolved: Some(resolved), .. } = &expression.kind
    else {
        panic!("expected resolved plain call");
    };

    assert!(matches!(&callee.kind, HirExprKind::Path(path) if path.to_string() == "ready"));
    assert_eq!(resolved.domain, HirCallableDomain::Function);
    assert_eq!(resolved.symbol.to_string(), "ready");
    assert_eq!(resolved.return_type, ValkyrieType::Boolean);
}

#[test]
fn resolves_point_call_as_constructor_in_hir_metadata() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4105 });
    let hir = compiler
        .compile_source(
            r#"
class Point {
    x: i64;
    y: i64;
}

micro main(x: i64, y: i64) -> Point {
    return Point(x, y);
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[0].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
    else {
        panic!("expected resolved constructor");
    };

    assert_eq!(resolved.domain, HirCallableDomain::Constructor);
    assert_eq!(resolved.symbol.to_string(), "Point");
    assert_eq!(resolved.return_type, ValkyrieType::Named(nyar_language::types::Identifier::new("Point")));
}

#[test]
fn resolves_operator_sugar_into_hir_metadata() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4102 });
    let hir = compiler
        .compile_source(
            r#"
class Vec {
    infix `+`(other: Vec) -> Vec {
        return self;
    }

    prefix `-`() -> Vec {
        return self;
    }

    suffix `[]`(ordinal: i64) -> i64 {
        return ordinal;
    }
}

micro add(left: Vec, right: Vec) -> Vec {
    return left + right;
}

micro negate(value: Vec) -> Vec {
    return -value;
}

micro get(buffer: Vec) -> i64 {
    return buffer[1];
}
"#,
        )
        .unwrap();

    let add_call = match &hir.functions[0].body.statements[0].kind {
        HirStatementKind::Expr(statement) => match &statement.kind {
            HirExprKind::Return(Some(expression)) => expression,
            _ => panic!("expected return expression"),
        },
        _ => panic!("expected expression statement"),
    };
    let HirExprKind::Call { resolved: Some(add_resolved), .. } = &add_call.kind
    else {
        panic!("expected resolved infix call");
    };
    assert_eq!(add_resolved.domain, HirCallableDomain::Operator);
    assert_eq!(add_resolved.symbol.to_string(), "infix +");

    let negate_call = match &hir.functions[1].body.statements[0].kind {
        HirStatementKind::Expr(statement) => match &statement.kind {
            HirExprKind::Return(Some(expression)) => expression,
            _ => panic!("expected return expression"),
        },
        _ => panic!("expected expression statement"),
    };
    let HirExprKind::Call { resolved: Some(negate_resolved), .. } = &negate_call.kind
    else {
        panic!("expected resolved prefix call");
    };
    assert_eq!(negate_resolved.domain, HirCallableDomain::Operator);
    assert_eq!(negate_resolved.symbol.to_string(), "prefix -");

    let get_call = match &hir.functions[2].body.statements[0].kind {
        HirStatementKind::Expr(statement) => match &statement.kind {
            HirExprKind::Return(Some(expression)) => expression,
            _ => panic!("expected return expression"),
        },
        _ => panic!("expected expression statement"),
    };
    let HirExprKind::Call { resolved: Some(get_resolved), .. } = &get_call.kind
    else {
        panic!("expected resolved subscript call");
    };
    assert_eq!(get_resolved.domain, HirCallableDomain::Operator);
    assert_eq!(get_resolved.symbol.to_string(), "suffix []");
}

#[test]
fn lowers_constructor_pattern_into_canonical_extractor_callee() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4103 });
    let hir = compiler
        .compile_source(
            r#"
class Point {
    micro extractor(self) -> (bool, bool)? {
        return null;
    }
}

micro main(value: Point) -> bool {
    return match value {
        case Point(flag, [1, ..rest]):
            flag
        else:
            false
    };
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[0].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Match { arms, .. } = &expression.kind
    else {
        panic!("expected match expression");
    };

    let HirPattern::Extractor(HirExtractorPattern::Constructor { name, canonical_callee, fields, resolved: Some(_) }) = &arms[0].pattern
    else {
        panic!("expected constructor extractor pattern");
    };
    assert_eq!(name.to_string(), "Point");
    assert_eq!(canonical_callee.parts().len(), 2);
    assert_eq!(canonical_callee.parts()[0].as_str(), "Point");
    assert_eq!(canonical_callee.parts()[1].as_str(), "extractor");
    assert!(matches!(fields.first(), Some(HirPattern::Variable(flag)) if flag.name.as_str() == "flag"));
}

#[test]
fn resolves_constructor_pattern_into_hir_extractor_metadata() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4106 });
    let hir = compiler
        .compile_source(
            r#"
class Point {
    micro extractor() -> (bool, bool)? {
        return (true, true);
    }
}

micro main(value: Point) -> bool {
    return match value {
        case Point(flag):
            flag
        else:
            false
    };
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[0].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Match { arms, .. } = &expression.kind
    else {
        panic!("expected match expression");
    };
    let HirPattern::Extractor(HirExtractorPattern::Constructor { resolved: Some(resolved), .. }) = &arms[0].pattern
    else {
        panic!("expected resolved extractor pattern");
    };

    assert_eq!(resolved.domain, HirCallableDomain::Extractor);
    assert_eq!(resolved.symbol.to_string(), "extractor");
    assert_eq!(
        resolved.return_type,
        ValkyrieType::Union(vec![
            ValkyrieType::Tuple(vec![ValkyrieType::Boolean, ValkyrieType::Boolean]),
            ValkyrieType::Named(Identifier::new("null")),
        ])
    );
    assert_eq!(resolved.extractor_payload_type, Some(ValkyrieType::Tuple(vec![ValkyrieType::Boolean, ValkyrieType::Boolean])));
}

#[test]
fn resolves_anonymous_row_parameter_call_into_hir_metadata() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4107 });
    let hir = compiler
        .compile_source(
            r#"
class Clock {
    micro now() -> i64 {
        return 1;
    }
}

micro read_now(value: { now() -> i64 }) -> i64 {
    return value.now();
}

micro main(value: Clock) -> i64 {
    return read_now(value);
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[1].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
    else {
        panic!("expected resolved row call");
    };

    assert_eq!(resolved.domain, HirCallableDomain::Function);
    assert_eq!(resolved.symbol.to_string(), "read_now");
    assert_eq!(resolved.return_type, ValkyrieType::Integer64 { signed: true });
}

#[test]
fn resolves_named_trait_parameter_over_row_in_hir_metadata() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4108 });
    let hir = compiler
        .compile_source(
            r#"
trait Writer {
    micro write(text: utf8);
}

class Console {
    micro write(text: utf8) {}
}

micro select(value: Writer) -> bool {
    return true;
}

micro select(value: { write(utf8) -> unit }) -> i64 {
    return 0;
}

micro main(value: Console) -> bool {
    return select(value);
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[2].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
    else {
        panic!("expected resolved trait call");
    };

    assert_eq!(resolved.domain, HirCallableDomain::Function);
    assert_eq!(resolved.symbol.to_string(), "select");
    assert_eq!(resolved.return_type, ValkyrieType::Boolean);
}

#[test]
fn resolves_nominal_subtype_parameter_over_trait_and_row_in_hir_metadata() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4109 });
    let hir = compiler
        .compile_source(
            r#"
trait Writer {
    micro write(text: utf8);
}

class Animal {}

class ServiceDog(Animal) {
    micro write(text: utf8) {}
}

micro choose(value: Animal) -> i64 {
    return 1;
}

micro choose(value: Writer) -> bool {
    return true;
}

micro choose(value: { write(utf8) -> unit }) -> utf8 {
    return "row";
}

micro main(value: ServiceDog) -> i64 {
    return choose(value);
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[3].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
    else {
        panic!("expected resolved nominal-subtype call");
    };

    assert_eq!(resolved.domain, HirCallableDomain::Function);
    assert_eq!(resolved.symbol.to_string(), "choose");
    assert_eq!(resolved.return_type, ValkyrieType::Integer64 { signed: true });
}

#[test]
fn resolves_nominal_exact_parameter_over_subtype_trait_and_row_in_hir_metadata() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4110 });
    let hir = compiler
        .compile_source(
            r#"
trait Writer {
    micro write(text: utf8);
}

class Animal {}

class Dog(Animal) {
    micro write(text: utf8) {}
}

micro choose(value: Dog) -> bool {
    return true;
}

micro choose(value: Animal) -> i64 {
    return 1;
}

micro choose(value: Writer) -> utf8 {
    return "trait";
}

micro choose(value: { write(utf8) -> unit }) -> [i64] {
    return [1];
}

micro main(value: Dog) -> bool {
    return choose(value);
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[4].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
    else {
        panic!("expected resolved nominal-exact call");
    };

    assert_eq!(resolved.domain, HirCallableDomain::Function);
    assert_eq!(resolved.symbol.to_string(), "choose");
    assert_eq!(resolved.return_type, ValkyrieType::Boolean);
}

#[test]
fn rejects_mut_self_pattern_extractor_at_compile_time() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4110 });
    let error = compiler
        .compile_source(
            r#"
class Point {
    micro extractor(mut self) -> bool? {
        return null;
    }
}

micro main(value: Point) -> unit {
    match value {
        case Point(_):
            ()
        else:
            ()
    }
}
"#,
        )
        .expect_err("mut self pattern extractor should fail at compile time");
    assert!(error.to_string().contains("mut self"));
}

#[test]
fn rejects_non_nullable_pattern_extractor_return_at_compile_time() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4111 });
    let error = compiler
        .compile_source(
            r#"
class Point {
    micro extractor(self) -> bool {
        return true;
    }
}

micro main(value: Point) -> unit {
    match value {
        case Point(_):
            ()
        else:
            ()
    }
}
"#,
        )
        .expect_err("non-nullable pattern extractor return should fail at compile time");
    assert!(error.to_string().contains("nullable"));
}

#[test]
fn rejects_unknown_pattern_extractor_at_compile_time() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4112 });
    let error = compiler
        .compile_source(
            r#"
class Point {
    micro extractor(self) -> bool? {
        return null;
    }
}

micro main(value: i64) -> unit {
    match value {
        case Point(_):
            ()
        else:
            ()
    }
}
"#,
        )
        .expect_err("unknown pattern extractor should fail at compile time");
    assert!(error.to_string().contains("unknown pattern extractor"));
}

#[test]
fn resolves_singleton_static_method_call_into_hir_metadata() {
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

    let main = hir.functions.iter().find(|function| function.name.as_str() == "main").expect("main function");
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
fn resolves_singleton_field_access_type_for_condition() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4202 });
    let hir = compiler
        .compile_source(
            r#"
singleton AppConfig {
    mut debug_mode: bool = false
}

micro main() -> bool {
    return AppConfig.debug_mode;
}
"#,
        )
        .unwrap();

    let main = hir.functions.iter().find(|function| function.name.as_str() == "main").expect("main function");
    let HirStatementKind::Expr(statement) = &main.body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::FieldAccess { object, field } = &expression.kind
    else {
        panic!("expected singleton field access");
    };
    let HirExprKind::Variable(identifier) = &object.kind
    else {
        panic!("expected singleton name object");
    };

    assert_eq!(identifier.name.as_str(), "AppConfig");
    assert_eq!(field.as_str(), "debug_mode");
}

#[test]
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

    let tick = hir.singletons[0].methods.iter().find(|method| method.name.as_str() == "tick").expect("tick method");
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

    let current = hir.singletons[0].methods.iter().find(|method| method.name.as_str() == "current").expect("current method");
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
    assert_eq!(resolved.symbol.to_string(), "choose");
    assert_eq!(resolved.return_type, ValkyrieType::Integer64 { signed: true });
}
