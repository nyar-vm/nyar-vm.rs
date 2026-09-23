use nyar_language::{SourceID, ValkyrieCompiler};

#[test]
fn rejects_let_with_refutable_pattern() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    let error = compiler
        .compile_source(
            r#"
class Option<T> {
}

class Some<T>(Option<T>) {
    value: T
}

class None<T>(Option<T>) {
}

micro describe(opt: Option<i64>) -> utf8 {
    let Some(x) = opt
    "ok"
}
"#,
        )
        .expect_err("`let Some(x) = opt` is a refutable pattern and should fail at type-check");
    let message = error.to_string();
    assert!(message.contains("refutable pattern"), "unexpected error: {message}");
}

#[test]
fn accepts_let_with_irrefutable_pattern() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    compiler
        .compile_source(
            r#"
micro describe(pair: (i64, i64)) -> utf8 {
    let (a, b) = pair
    "ok"
}
"#,
        )
        .expect("`let (a, b) = pair` is irrefutable and should compile");
}

#[test]
fn accepts_if_let_with_refutable_pattern() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    compiler
        .compile_source(
            r#"
class Option<T> {
}

class Some<T>(Option<T>) {
    value: T
}

class None<T>(Option<T>) {
}

micro describe(opt: Option<i64>) -> utf8 {
    if let Some(x) = opt {
        "some"
    } else {
        "none"
    }
}
"#,
        )
        .expect("`if let Some(x) = opt` should compile");
}

#[test]
fn rejects_let_with_literal_pattern() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    let error = compiler
        .compile_source(
            r#"
micro describe(value: i64) -> utf8 {
    let 42 = value
    "ok"
}
"#,
        )
        .expect_err("`let 42 = value` is a refutable literal pattern and should fail");
    let message = error.to_string();
    assert!(message.contains("refutable pattern"), "unexpected error: {message}");
}
