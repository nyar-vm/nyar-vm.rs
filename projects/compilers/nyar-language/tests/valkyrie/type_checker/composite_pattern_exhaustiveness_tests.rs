use nyar_language::{SourceID, ValkyrieCompiler};

/// 当所有 arm 都是 extractor pattern 且没有 wildcard/else 兜底时，应报 non-exhaustive。
#[test]
fn extractor_only_arms_without_wildcard_reports_non_exhaustive() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    let error = compiler
        .compile_source(
            r#"
class Wrapper {
    micro extractor(self) -> (i64, i64)? {
        return null;
    }
}

micro describe(value: Wrapper) -> bool {
    match value {
        case Wrapper(a, b):
            true
    }
}
"#,
        )
        .expect_err("extractor-only arms without wildcard should be non-exhaustive");
    let message = error.to_string();
    assert!(message.contains("non-exhaustive"), "unexpected error: {message}");
}

/// extractor arm 配合 `else` 兜底 arm 时，match 穷尽，应编译通过。
#[test]
fn extractor_arms_with_wildcard_passes() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    compiler
        .compile_source(
            r#"
class Wrapper {
    micro extractor(self) -> (i64, i64)? {
        return null;
    }
}

micro describe(value: Wrapper) -> bool {
    match value {
        case Wrapper(a, b):
            true
        else:
            false
    }
}
"#,
        )
        .expect("extractor arms with else fallback should be exhaustive");
}

/// scrutinee 为 tuple 类型，arm 为全变量 tuple pattern（irrefutable），应编译通过。
#[test]
fn irrefutable_tuple_arms_passes() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    compiler
        .compile_source(
            r#"
micro describe(pair: (i64, i64)) -> bool {
    match pair {
        case (a, b):
            true
    }
}
"#,
        )
        .expect("irrefutable tuple arm should be exhaustive");
}

/// scrutinee 为 tuple 类型，arm 含 literal 字段（refutable）且无 wildcard，应报 non-exhaustive。
#[test]
fn tuple_with_literal_field_without_wildcard_reports_non_exhaustive() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    let error = compiler
        .compile_source(
            r#"
micro describe(pair: (i64, i64)) -> bool {
    match pair {
        case (_, 0):
            true
    }
}
"#,
        )
        .expect_err("tuple with literal field and no wildcard should be non-exhaustive");
    let message = error.to_string();
    assert!(message.contains("non-exhaustive"), "unexpected error: {message}");
}

/// scrutinee 为 class 类型，arm 为无 rest binding 的 object pattern（refutable），应报 non-exhaustive。
#[test]
fn object_pattern_without_rest_binding_reports_non_exhaustive() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    let error = compiler
        .compile_source(
            r#"
class Holder {
    value: i64;
}

micro describe(value: Holder) -> bool {
    match value {
        case Holder { value: n }:
            true
    }
}
"#,
        )
        .expect_err("object pattern without rest binding should be non-exhaustive");
    let message = error.to_string();
    assert!(message.contains("non-exhaustive"), "unexpected error: {message}");
}

/// scrutinee 为 class 类型，arm 为带 rest binding 的 object pattern（irrefutable），应编译通过。
#[test]
fn object_pattern_with_rest_binding_passes() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    compiler
        .compile_source(
            r#"
class Holder {
    value: i64;
}

micro describe(value: Holder) -> bool {
    match value {
        case Holder { value: n, ...rest }:
            true
    }
}
"#,
        )
        .expect("object pattern with rest binding should be exhaustive");
}

/// irrefutable pattern 加 guard 不算无条件覆盖，无 wildcard 时应报 non-exhaustive。
#[test]
fn guard_on_irrefutable_does_not_count_as_full_coverage() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    let error = compiler
        .compile_source(
            r#"
micro describe(pair: (i64, i64)) -> bool {
    match pair {
        case (a, b) if a > 0:
            true
    }
}
"#,
        )
        .expect_err("guarded irrefutable arm without wildcard should be non-exhaustive");
    let message = error.to_string();
    assert!(message.contains("non-exhaustive"), "unexpected error: {message}");
}

/// 多个 irrefutable arm（均为全变量 tuple pattern）时，match 穷尽，应编译通过。
#[test]
fn all_irrefutable_arms_passes() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    compiler
        .compile_source(
            r#"
micro describe(pair: (i64, i64)) -> bool {
    match pair {
        case (a, b):
            true
        case (c, d):
            false
    }
}
"#,
        )
        .expect("all irrefutable arms should be exhaustive");
}
