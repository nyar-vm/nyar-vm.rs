use nyar_language::{SourceID, ValkyrieCompiler};

/// A sealed class with two subclasses covered only by guarded arms must still be reported as
/// non-exhaustive, because a guard may fail at runtime and fall through.
#[test]
fn guard_does_not_count_as_full_coverage() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
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
        case Circle if true:
            "circle"
        case Rectangle if true:
            "rectangle"
    }
}
"#,
        )
        .expect_err("guarded arms should not count as full coverage and must report non-exhaustive");
    let message = error.to_string();
    assert!(message.contains("non exhaustive match"), "unexpected error: {message}");
}

/// The same sealed class matched without guards over all subclasses is exhaustive and compiles.
#[test]
fn unguarded_arms_still_exhaustive() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    compiler
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
        case Rectangle:
            "rectangle"
    }
}
"#,
        )
        .expect("unguarded arms covering every subclass should compile without error");
}

/// An `Or` pattern that repeats a variant must be flagged as a duplicate match arm, with each
/// sub-pattern contributing its variant name to the duplicate scan.
#[test]
fn or_pattern_duplicate_detection() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
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
        case Circle | Circle | Rectangle:
            "ok"
    }
}
"#,
        )
        .expect_err("repeated variant inside an Or pattern should be reported as a duplicate arm");
    let message = error.to_string();
    assert!(message.contains("duplicate"), "unexpected error: {message}");
}
