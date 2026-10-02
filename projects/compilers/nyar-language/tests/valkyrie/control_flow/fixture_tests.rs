use super::fixtures::{
    EXPLICIT_RETURN, NULLABLE_TRY_PROPAGATE, assert_nullable_try_mir_shape, assert_return_mir_shape,
    compile_fixture,
};

#[test]
fn return_fixture_has_mir_return_terminator() {
    let hir = compile_fixture(EXPLICIT_RETURN);
    assert_return_mir_shape(&hir);
}

#[test]
fn nullable_try_fixture_has_branch_and_unwrap_blocks() {
    let hir = compile_fixture(NULLABLE_TRY_PROPAGATE);
    assert_nullable_try_mir_shape(&hir);
}
