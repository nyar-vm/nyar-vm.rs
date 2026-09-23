use nyar_language::{SourceID, ValkyrieCompiler};

/// `case _ => ... case 1 => ...` 中无条件 wildcard 完全覆盖，后续 `case 1` 不可达。
#[test]
fn wildcard_makes_subsequent_arms_unreachable() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4300 });
    let error = compiler
        .compile_source(
            r#"
micro describe(n: i32) -> utf8 {
    match n {
        case _:
            "wild"
        case 1:
            "one"
    }
}
"#,
        )
        .expect_err("an arm after an unguarded wildcard should be unreachable");
    let message = error.to_string();
    assert!(message.contains("unreachable"), "unexpected error: {message}");
}

/// `case 1 => ... case 1 => ...` 相同字面量出现两次，第二个 arm 不可达。
#[test]
fn literal_after_same_literal_is_unreachable() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4301 });
    let error = compiler
        .compile_source(
            r#"
micro describe(n: i32) -> utf8 {
    match n {
        case 1:
            "one"
        case 1:
            "again"
    }
}
"#,
        )
        .expect_err("a literal arm following the same literal should be unreachable");
    let message = error.to_string();
    assert!(message.contains("unreachable"), "unexpected error: {message}");
}

/// `case 1 | 2 => ... case 1 => ...` 中 Or pattern 已覆盖 1，后续 `case 1` 不可达。
#[test]
fn or_pattern_covers_subsequent_literal() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4302 });
    let error = compiler
        .compile_source(
            r#"
micro describe(n: i32) -> utf8 {
    match n {
        case 1 | 2:
            "low"
        case 1:
            "one"
    }
}
"#,
        )
        .expect_err("a literal already covered by a prior Or pattern should be unreachable");
    let message = error.to_string();
    assert!(message.contains("unreachable"), "unexpected error: {message}");
}

/// sealed 类型 `case A => ... case A => ... case B => ...` 中重复的 `A` arm 不可达。
#[test]
fn sealed_variant_duplicate_is_unreachable() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4303 });
    let error = compiler
        .compile_source(
            r#"
sealed class Shape {
}

class Circle(Shape) {
}

class Rectangle(Shape) {
}

micro describe(shape: Shape) -> utf8 {
    match shape {
        case Circle:
            "circle"
        case Circle:
            "again"
        case Rectangle:
            "rectangle"
    }
}
"#,
        )
        .expect_err("a sealed variant arm following the same variant should be unreachable");
    let message = error.to_string();
    assert!(message.contains("unreachable"), "unexpected error: {message}");
}

/// `case 1 => ... case 2 => ... case _ => ...` 各 arm 均可达，不应报 unreachable。
#[test]
fn reachable_arms_not_flagged() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4304 });
    compiler
        .compile_source(
            r#"
micro describe(n: i32) -> utf8 {
    match n {
        case 1:
            "one"
        case 2:
            "two"
        case _:
            "other"
    }
}
"#,
        )
        .expect("distinct literal arms followed by a wildcard should not be flagged unreachable");
}

/// `case 1 if cond => ... case 1 => ...` 中第一个 arm 带 guard，guard 可能失败，
/// 因此第二个 `case 1` 仍可达，不应被报为 unreachable。
#[test]
fn guard_arm_does_not_make_subsequent_unreachable() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4305 });
    let result = compiler.compile_source(
        r#"
micro describe(n: i32) -> utf8 {
    match n {
        case 1 if true:
            "one"
        case 1:
            "again"
        case _:
            "other"
    }
}
"#,
    );
    match result {
        Ok(_) => { /* 无任何错误则自然满足：未被报 unreachable */ }
        Err(error) => {
            let message = error.to_string();
            assert!(
                !message.contains("unreachable"),
                "guarded arm should not make subsequent same-pattern arm unreachable, but got: {message}"
            );
        }
    }
}

/// `case _ if cond => ... case 1 => ...` 中带 guard 的 wildcard 不构成无条件覆盖，
/// guard 可能失败，因此后续 `case 1` 仍可达，不应被报为 unreachable。
#[test]
fn wildcard_with_guard_does_not_cover_all() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4306 });
    let result = compiler.compile_source(
        r#"
micro describe(n: i32) -> utf8 {
    match n {
        case _ if true:
            "guard"
        case 1:
            "one"
        case _:
            "other"
    }
}
"#,
    );
    match result {
        Ok(_) => { /* 无任何错误则自然满足：未被报 unreachable */ }
        Err(error) => {
            let message = error.to_string();
            assert!(!message.contains("unreachable"), "guarded wildcard should not make subsequent arm unreachable, but got: {message}");
        }
    }
}

/// 同变体不同字面量载荷（如 `Punctuation("{")` / `Punctuation("}")`）均可达，不得误报 unreachable。
#[test]
fn distinct_constructor_payloads_are_reachable() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4307 });
    let result = compiler.compile_source(
        r#"
unite Tok {
    Punctuation { text: utf8 }
    Other
}

micro describe(t: Tok) -> utf8 {
    match t {
        case Punctuation("{"):
            "open"
        case Punctuation("}"):
            "close"
        case Punctuation(text):
            text
        case Other:
            "other"
    }
}
"#,
    );
    match result {
        Ok(_) => {}
        Err(error) => {
            let message = error.to_string();
            assert!(!message.contains("unreachable"), "distinct constructor payloads must stay reachable, but got: {message}");
        }
    }
}
