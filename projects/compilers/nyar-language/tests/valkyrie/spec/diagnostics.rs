use nyar_language::{
    types::{Identifier, NamePath},
    valkyrie::{
        diagnostics::{SemanticFailure, diagnose_failure},
        row::RowRequirementError,
        trait_system::TraitSatisfactionError,
    },
};

#[test]
fn diagnostics_distinguish_nominal_and_row_failure() {
    let nominal =
        diagnose_failure(&SemanticFailure::NominalMismatch { expected: Identifier::new("Animal"), found: Identifier::new("RobotDog") });
    let row = diagnose_failure(&SemanticFailure::Row { error: RowRequirementError::MissingMethod { name: Identifier::new("size") } });

    assert_ne!(nominal.code, row.code);
    assert!(nominal.message.contains("名义类型"));
    assert!(row.message.contains("匿名 row"));
}

#[test]
fn diagnostics_distinguish_row_and_trait_failure() {
    let row = diagnose_failure(&SemanticFailure::Row { error: RowRequirementError::MissingMethod { name: Identifier::new("read") } });
    let trait_failure = diagnose_failure(&SemanticFailure::Trait {
        error: TraitSatisfactionError::MissingExplicitImplForAssociatedTypes { trait_path: NamePath::new(vec![Identifier::new("Iterator")]) },
    });

    assert_ne!(row.code, trait_failure.code);
    assert!(row.message.contains("匿名 row"));
    assert!(trait_failure.message.contains("具名 trait"));
}

#[test]
fn diagnostics_distinguish_effect_failure() {
    let effect = diagnose_failure(&SemanticFailure::Effect { effect_name: NamePath::new(vec![Identifier::new("IO")]) });
    let trait_failure = diagnose_failure(&SemanticFailure::Trait {
        error: TraitSatisfactionError::StructuralMismatch {
            errors: vec![RowRequirementError::MissingMethod { name: Identifier::new("handle") }],
        },
    });

    assert_ne!(effect.code, trait_failure.code);
    assert!(effect.message.contains("effect"));
    assert!(trait_failure.message.contains("witness"));
}

#[test]
fn diagnostics_distinguish_trait_ambiguity_from_trait_mismatch() {
    let trait_ambiguity = diagnose_failure(&SemanticFailure::Trait {
        error: TraitSatisfactionError::StructuralAmbiguity {
            errors: vec![RowRequirementError::AmbiguousCandidateMethod { name: Identifier::new("write") }],
        },
    });
    let trait_mismatch = diagnose_failure(&SemanticFailure::Trait {
        error: TraitSatisfactionError::StructuralMismatch {
            errors: vec![RowRequirementError::MissingMethod { name: Identifier::new("write") }],
        },
    });

    assert_ne!(trait_ambiguity.code, trait_mismatch.code);
    assert!(trait_ambiguity.message.contains("歧义"));
    assert!(trait_mismatch.message.contains("结构入口"));
}

#[test]
fn diagnostics_distinguish_super_trait_requirement() {
    let diagnostic = diagnose_failure(&SemanticFailure::Trait {
        error: TraitSatisfactionError::MissingExplicitImplForSuperTraits {
            trait_path: NamePath::new(vec![Identifier::new("BufferedReadable")]),
        },
    });

    assert_eq!(diagnostic.code, "E_TRAIT_SUPER_IMPL_REQUIRED");
    assert!(diagnostic.message.contains("super trait"));
}
