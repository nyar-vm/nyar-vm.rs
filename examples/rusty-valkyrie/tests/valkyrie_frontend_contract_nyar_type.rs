use nyar::{NyarFunctionType, NyarType};
use rusty_valkyrie::{
    concretize_type,
    types::{
        Identifier,
        hir::{FunctionType, GenericType, HirKind, ValkyrieType},
    },
};

#[test]
fn concretizes_scalars_and_arrays() {
    let ty = ValkyrieType::Array(Box::new(ValkyrieType::Integer32 { signed: true }));
    assert_eq!(concretize_type(&ty).unwrap(), NyarType::Array(Box::new(NyarType::Integer32 { signed: true })));
}

#[test]
fn rejects_named_array_apply_short_name() {
    let ty = ValkyrieType::Apply(Box::new(ValkyrieType::Named(nyar::Identifier::new("Array"))), vec![ValkyrieType::Integer32 { signed: true }]);
    assert!(concretize_type(&ty).is_err(), "Apply(Named(\"Array\"), …) 不得降为平台 Array");
}

#[test]
fn rejects_auto_and_generic() {
    assert!(concretize_type(&ValkyrieType::AutoType).is_err());
    assert!(
        concretize_type(&ValkyrieType::Generic(GenericType {
            binder: None,
            name: nyar::Identifier::new("T"),
            kind: HirKind::Type,
            bounds: Vec::new()
        }))
        .is_err()
    );
}

#[test]
fn rejects_named_self_as_unsubstituted_stain() {
    assert!(concretize_type(&ValkyrieType::SelfType).is_err());
    assert!(concretize_type(&ValkyrieType::Named(nyar::Identifier::new("Self"))).is_err());
}

#[test]
fn rejects_row_and_frontend_only_constructs() {
    assert!(concretize_type(&ValkyrieType::Row(rusty_valkyrie::types::hir::RowType { methods: Vec::new() })).is_err());
    assert!(concretize_type(&ValkyrieType::Intersection(Vec::new())).is_err());
}

#[test]
fn concretizes_function_type() {
    let ty = ValkyrieType::Function(Box::new(FunctionType { params: vec![ValkyrieType::Boolean], return_type: ValkyrieType::Unit }));
    assert_eq!(
        concretize_type(&ty).unwrap(),
        NyarType::Function(Box::new(NyarFunctionType { params: vec![NyarType::Boolean], return_type: NyarType::Unit }))
    );
}
