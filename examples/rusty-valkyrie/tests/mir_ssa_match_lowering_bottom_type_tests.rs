use rusty_valkyrie::{
    mir::{MirConstant, MirOperand},
    types::hir::ValkyrieType,
};

use rusty_valkyrie::mir::ssa::arm_contributes_to_match_merge;

#[test]
fn only_structured_bottom_excludes_an_arm_from_merge() {
    let result = MirOperand::Constant(MirConstant::Int(1));
    assert!(!arm_contributes_to_match_merge(&result, Some(&ValkyrieType::Void), None));
    let ordinary_named_type = ValkyrieType::Named(rusty_valkyrie::types::Identifier::new("Never"));
    assert!(arm_contributes_to_match_merge(&result, Some(&ordinary_named_type), None));
}
