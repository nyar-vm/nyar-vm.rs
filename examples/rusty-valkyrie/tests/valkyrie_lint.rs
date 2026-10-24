use rusty_valkyrie::*;

#[test]
fn lint_accepts_minimal_function_without_entering_artifact_pipeline() {
    lint_valkyrie_source(&ValkyrieCompiler::default(), "micro identity(value: i32) -> i32 { return value }")
        .expect("HIR lint must not require resolver source groups");
}
