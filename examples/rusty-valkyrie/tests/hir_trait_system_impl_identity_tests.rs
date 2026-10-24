use rusty_valkyrie::{
    types::hir::{HirAssociatedTypeBinding, HirImpl, HirTraitBound, HirWhereConstraint, ValkyrieType as HirType},
    *,
};

use rusty_valkyrie::hir::trait_system::{ImplSpecificity, compare_impl_specificity};

#[test]
fn associated_equations_participate_in_impl_specificity() {
    let trait_path = NamePath::new(vec![Identifier::new("Iterator")]);
    let left = HirImpl {
        where_constraints: vec![HirWhereConstraint {
            target: HirType::Named(Identifier::new("T")),
            bounds: vec![HirTraitBound {
                declaration: None,
                trait_path,
                type_arguments: Vec::new(),
                associated_types: vec![HirAssociatedTypeBinding { name: Identifier::new("Item"), ty: HirType::Integer32 { signed: true } }],
            }],
            span: SourceSpan::new(SourceID::default(), 0, 0),
        }],
        ..HirImpl::default()
    };
    let mut right = left.clone();
    assert_eq!(compare_impl_specificity(&left, &right), ImplSpecificity::Equivalent);
    right.where_constraints[0].bounds[0].associated_types[0].ty = HirType::Boolean;
    assert_eq!(compare_impl_specificity(&left, &right), ImplSpecificity::Incomparable);
}

#[test]
fn positional_arguments_participate_in_impl_specificity() {
    let left = HirImpl {
        where_constraints: vec![HirWhereConstraint {
            target: HirType::Named(Identifier::new("T")),
            bounds: vec![HirTraitBound {
                declaration: None,
                trait_path: NamePath::new(vec![Identifier::new("Mapper")]),
                type_arguments: vec![HirType::Boolean],
                associated_types: Vec::new(),
            }],
            span: SourceSpan::new(SourceID::default(), 0, 0),
        }],
        ..HirImpl::default()
    };
    let mut right = left.clone();
    right.where_constraints[0].bounds[0].type_arguments[0] = HirType::Integer32 { signed: true };
    assert_eq!(compare_impl_specificity(&left, &right), ImplSpecificity::Incomparable);
}
