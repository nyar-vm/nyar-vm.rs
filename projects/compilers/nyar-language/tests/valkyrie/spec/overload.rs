use nyar_language::{
    types::{Identifier, NamePath, hir::ValkyrieType},
    valkyrie::overload::{OverloadCandidate, OverloadDomain, OverloadMatchKind, OverloadResolutionError, resolve_overload},
};

#[test]
fn overload_prefers_nominal_then_trait_then_row() {
    let exact = resolve_overload(&[
        candidate("take_animal_exact", OverloadMatchKind::NominalExact),
        candidate("take_animal_sub", OverloadMatchKind::NominalSubtype { distance: 1 }),
        candidate("take_reader_trait", OverloadMatchKind::Trait),
        candidate("take_row", OverloadMatchKind::Row),
    ])
    .unwrap();

    let subtype = resolve_overload(&[
        candidate("take_animal_sub", OverloadMatchKind::NominalSubtype { distance: 1 }),
        candidate("take_reader_trait", OverloadMatchKind::Trait),
        candidate("take_row", OverloadMatchKind::Row),
    ])
    .unwrap();

    let trait_match =
        resolve_overload(&[candidate("take_reader_trait", OverloadMatchKind::Trait), candidate("take_row", OverloadMatchKind::Row)]).unwrap();

    assert_eq!(exact.symbol, path("take_animal_exact"));
    assert_eq!(subtype.symbol, path("take_animal_sub"));
    assert_eq!(trait_match.symbol, path("take_reader_trait"));
}

#[test]
fn row_only_overloads_are_ambiguous() {
    let error =
        resolve_overload(&[candidate("take_row_a", OverloadMatchKind::Row), candidate("take_row_b", OverloadMatchKind::Row)]).unwrap_err();

    assert_eq!(error, OverloadResolutionError::Ambiguous { candidates: vec![path("take_row_a"), path("take_row_b")] });
}

#[test]
fn same_rank_trait_overloads_are_ambiguous() {
    let error = resolve_overload(&[candidate("take_readable", OverloadMatchKind::Trait), candidate("take_seekable", OverloadMatchKind::Trait)])
        .unwrap_err();

    assert_eq!(error, OverloadResolutionError::Ambiguous { candidates: vec![path("take_readable"), path("take_seekable")] });
}

#[test]
fn same_distance_nominal_subtypes_are_ambiguous() {
    let error = resolve_overload(&[
        candidate("take_mammal", OverloadMatchKind::NominalSubtype { distance: 1 }),
        candidate("take_pet", OverloadMatchKind::NominalSubtype { distance: 1 }),
    ])
    .unwrap_err();

    assert_eq!(error, OverloadResolutionError::Ambiguous { candidates: vec![path("take_mammal"), path("take_pet")] });
}

fn candidate(name: &str, match_kind: OverloadMatchKind) -> OverloadCandidate {
    OverloadCandidate::new(path(name), OverloadDomain::Function, vec![], ValkyrieType::Unit, match_kind)
}

fn path(name: &str) -> NamePath {
    NamePath::new(vec![Identifier::new(name)])
}
