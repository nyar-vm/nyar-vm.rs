//! Tests for sealed class optimization module.

use nyar_language::{
    ExhaustivenessResult, SealedClassJumpTable, SealedClassMemoryLayout, SealedExhaustivenessChecker, TagType,
};
use nyar_language::types::Identifier;

fn create_identifier(name: &str) -> Identifier {
    Identifier::new(name)
}

#[test]
fn test_jump_table_creation() {
    let table = SealedClassJumpTable::new(create_identifier("Option"));
    assert!(table.is_empty());
    assert_eq!(table.len(), 0);
    assert_eq!(table.class_name.as_str(), "Option");
}

#[test]
fn test_jump_table_add_variants() {
    let mut table = SealedClassJumpTable::new(create_identifier("Option"));
    table.add_variant(create_identifier("Some"), "some_label".to_string());
    table.add_variant(create_identifier("None"), "none_label".to_string());

    assert_eq!(table.len(), 2);
    assert!(!table.is_empty());
}

#[test]
fn test_jump_table_exhaustiveness() {
    let mut table = SealedClassJumpTable::new(create_identifier("Option"));
    table.add_variant(create_identifier("Some"), "some_label".to_string());
    table.add_variant(create_identifier("None"), "none_label".to_string());

    let all_variants = vec![create_identifier("Some"), create_identifier("None")];

    assert!(table.is_exhaustive(&all_variants));
}

#[test]
fn test_jump_table_not_exhaustive() {
    let mut table = SealedClassJumpTable::new(create_identifier("Option"));
    table.add_variant(create_identifier("Some"), "some_label".to_string());

    let all_variants = vec![create_identifier("Some"), create_identifier("None")];

    assert!(!table.is_exhaustive(&all_variants));
}

#[test]
fn test_jump_table_missing_variants() {
    let mut table = SealedClassJumpTable::new(create_identifier("Result"));
    table.add_variant(create_identifier("Ok"), "ok_label".to_string());

    let all_variants = vec![
        create_identifier("Ok"),
        create_identifier("Err"),
        create_identifier("Timeout"),
    ];

    let missing = table.missing_variants(&all_variants);
    assert_eq!(missing.len(), 2);
}

#[test]
fn test_jump_table_lookup() {
    let mut table = SealedClassJumpTable::new(create_identifier("Result"));
    table.add_variant(create_identifier("Ok"), "ok_label".to_string());
    table.add_variant(create_identifier("Err"), "err_label".to_string());

    assert_eq!(table.lookup(&create_identifier("Ok")), Some("ok_label"));
    assert_eq!(table.lookup(&create_identifier("Err")), Some("err_label"));
    assert_eq!(table.lookup(&create_identifier("Unknown")), None);
}

#[test]
fn test_jump_table_lookup_by_index() {
    let mut table = SealedClassJumpTable::new(create_identifier("Color"));
    table.add_variant(create_identifier("Red"), "red_label".to_string());
    table.add_variant(create_identifier("Green"), "green_label".to_string());
    table.add_variant(create_identifier("Blue"), "blue_label".to_string());

    assert_eq!(table.lookup_by_index(0), Some("red_label"));
    assert_eq!(table.lookup_by_index(1), Some("green_label"));
    assert_eq!(table.lookup_by_index(2), Some("blue_label"));
    assert_eq!(table.lookup_by_index(3), None);
}

#[test]
fn test_jump_table_has_default() {
    let mut table = SealedClassJumpTable::new(create_identifier("Option"));
    assert!(!table.has_default);

    table.set_has_default();
    assert!(table.has_default);
}

#[test]
fn test_tag_type_selection() {
    assert_eq!(TagType::select(1), TagType::U8);
    assert_eq!(TagType::select(100), TagType::U8);
    assert_eq!(TagType::select(256), TagType::U8);
    assert_eq!(TagType::select(257), TagType::U16);
    assert_eq!(TagType::select(1000), TagType::U16);
    assert_eq!(TagType::select(65536), TagType::U16);
    assert_eq!(TagType::select(65537), TagType::U32);
    assert_eq!(TagType::select(100000), TagType::U32);
    assert_eq!(TagType::select(4_294_967_296), TagType::U32);
    assert_eq!(TagType::select(4_294_967_297), TagType::U64);
}

#[test]
fn test_tag_type_size() {
    assert_eq!(TagType::U8.size(), 1);
    assert_eq!(TagType::U16.size(), 2);
    assert_eq!(TagType::U32.size(), 4);
    assert_eq!(TagType::U64.size(), 8);
}

#[test]
fn test_tag_type_alignment() {
    assert_eq!(TagType::U8.alignment(), 1);
    assert_eq!(TagType::U16.alignment(), 2);
    assert_eq!(TagType::U32.alignment(), 4);
    assert_eq!(TagType::U64.alignment(), 8);
}

#[test]
fn test_tag_type_default() {
    assert_eq!(TagType::default(), TagType::U8);
}

#[test]
fn test_memory_layout_creation() {
    let layout = SealedClassMemoryLayout::new(create_identifier("Option"));
    assert_eq!(layout.class_name.as_str(), "Option");
    assert_eq!(layout.tag_type, TagType::U8);
    assert_eq!(layout.tag_size, 1);
}

#[test]
fn test_memory_layout_with_tag_type() {
    let layout =
        SealedClassMemoryLayout::new(create_identifier("LargeEnum")).with_tag_type(TagType::U16);
    assert_eq!(layout.tag_type, TagType::U16);
    assert_eq!(layout.tag_size, 2);
}

#[test]
fn test_memory_layout_with_union_size() {
    let layout = SealedClassMemoryLayout::new(create_identifier("Data")).with_union_size(64);
    assert_eq!(layout.union_size, 64);
}

#[test]
fn test_memory_layout_with_alignment() {
    let layout = SealedClassMemoryLayout::new(create_identifier("Aligned")).with_alignment(16);
    assert_eq!(layout.alignment, 16);
}

#[test]
fn test_exhaustiveness_checker_creation() {
    let checker = SealedExhaustivenessChecker::new();
    assert!(checker.is_empty());
    assert_eq!(checker.len(), 0);
}

#[test]
fn test_exhaustiveness_checker_register() {
    let mut checker = SealedExhaustivenessChecker::new();
    checker.register(
        create_identifier("Option"),
        vec![create_identifier("Some"), create_identifier("None")],
    );

    assert!(!checker.is_empty());
    assert_eq!(checker.len(), 1);
    assert!(checker.is_sealed_class(&create_identifier("Option")));
}

#[test]
fn test_exhaustiveness_checker_get_variants() {
    let mut checker = SealedExhaustivenessChecker::new();
    checker.register(
        create_identifier("Option"),
        vec![create_identifier("Some"), create_identifier("None")],
    );

    let variants = checker.get_variants(&create_identifier("Option"));
    assert!(variants.is_some());
    let variants = variants.unwrap();
    assert_eq!(variants.len(), 2);
}

#[test]
fn test_exhaustiveness_checker_exhaustive() {
    let mut checker = SealedExhaustivenessChecker::new();
    checker.register(
        create_identifier("Option"),
        vec![create_identifier("Some"), create_identifier("None")],
    );

    let result = checker.check_exhaustiveness(
        &create_identifier("Option"),
        &[create_identifier("Some"), create_identifier("None")],
    );

    assert!(result.is_exhaustive());
    assert_eq!(result, ExhaustivenessResult::Exhaustive);
}

#[test]
fn test_exhaustiveness_checker_missing_variants() {
    let mut checker = SealedExhaustivenessChecker::new();
    checker.register(
        create_identifier("Option"),
        vec![create_identifier("Some"), create_identifier("None")],
    );

    let result =
        checker.check_exhaustiveness(&create_identifier("Option"), &[create_identifier("Some")]);

    assert!(result.has_missing());
    let missing = result.missing().unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].as_str(), "None");
}

#[test]
fn test_exhaustiveness_checker_not_sealed() {
    let checker = SealedExhaustivenessChecker::new();

    let result =
        checker.check_exhaustiveness(&create_identifier("NotSealed"), &[create_identifier("A")]);

    assert_eq!(result, ExhaustivenessResult::NotSealed);
}

#[test]
fn test_exhaustiveness_checker_wildcard() {
    let mut checker = SealedExhaustivenessChecker::new();
    checker.register(
        create_identifier("Option"),
        vec![create_identifier("Some"), create_identifier("None")],
    );

    assert!(checker.is_wildcard_exhaustive(&create_identifier("Option"), true));
    assert!(!checker.is_wildcard_exhaustive(&create_identifier("Option"), false));
    assert!(checker.is_wildcard_exhaustive(&create_identifier("NotSealed"), false));
}

#[test]
fn test_exhaustiveness_checker_clear() {
    let mut checker = SealedExhaustivenessChecker::new();
    checker.register(
        create_identifier("Option"),
        vec![create_identifier("Some"), create_identifier("None")],
    );

    assert_eq!(checker.len(), 1);
    checker.clear();
    assert!(checker.is_empty());
}

#[test]
fn test_exhaustiveness_checker_generate_jump_table() {
    let mut checker = SealedExhaustivenessChecker::new();
    checker.register(
        create_identifier("Option"),
        vec![create_identifier("Some"), create_identifier("None")],
    );

    let table = checker.generate_jump_table(
        &create_identifier("Option"),
        &[
            (create_identifier("Some"), "some_label".to_string()),
            (create_identifier("None"), "none_label".to_string()),
        ],
        false,
    );

    assert!(table.is_some());
    let table = table.unwrap();
    assert_eq!(table.len(), 2);
}

#[test]
fn test_exhaustiveness_checker_generate_jump_table_not_sealed() {
    let checker = SealedExhaustivenessChecker::new();

    let table = checker.generate_jump_table(
        &create_identifier("NotSealed"),
        &[(create_identifier("A"), "a_label".to_string())],
        false,
    );

    assert!(table.is_none());
}

#[test]
fn test_exhaustiveness_result_is_exhaustive() {
    assert!(ExhaustivenessResult::Exhaustive.is_exhaustive());
    assert!(!ExhaustivenessResult::MissingVariants(vec![]).is_exhaustive());
    assert!(!ExhaustivenessResult::NotSealed.is_exhaustive());
}

#[test]
fn test_exhaustiveness_result_has_missing() {
    assert!(!ExhaustivenessResult::Exhaustive.has_missing());
    assert!(ExhaustivenessResult::MissingVariants(vec![create_identifier("A")]).has_missing());
    assert!(!ExhaustivenessResult::NotSealed.has_missing());
}

#[test]
fn test_exhaustiveness_result_missing() {
    assert!(ExhaustivenessResult::Exhaustive.missing().is_none());
    assert!(ExhaustivenessResult::NotSealed.missing().is_none());

    let missing = vec![create_identifier("A"), create_identifier("B")];
    let result = ExhaustivenessResult::MissingVariants(missing.clone());
    let result_missing = result.missing().unwrap();
    assert_eq!(result_missing.len(), 2);
}
