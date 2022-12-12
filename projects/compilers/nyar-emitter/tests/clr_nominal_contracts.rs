use nyar_emitter::testing::build_clr_nominal_type_defs;
use nyar_language::{FlagsLayout, SumTypeLayout};

#[test]
fn sum_types_emit_tag_and_payload_fields() {
    let defs = build_clr_nominal_type_defs(
        &[SumTypeLayout { name: "OptionInt".to_string(), is_unite: false, tag_width: 4, variants: Vec::new() }],
        &[],
    );

    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].full_name, "OptionInt");
    assert!(defs[0].is_value_type);
    assert_eq!(defs[0].fields.len(), 2);
    assert_eq!(defs[0].fields[0].name, "tag");
    assert_eq!(defs[0].fields[1].name, "payload");
}

#[test]
fn unite_sum_types_emit_reference_classes_and_flags_emit_value_types() {
    let defs = build_clr_nominal_type_defs(
        &[SumTypeLayout { name: "Result".to_string(), is_unite: true, tag_width: 4, variants: Vec::new() }],
        &[FlagsLayout { name: "Permissions".to_string() }],
    );

    assert_eq!(defs.len(), 2);
    let result = defs.iter().find(|d| d.full_name == "Result").expect("Result");
    assert!(!result.is_value_type);
    assert_eq!(result.fields.len(), 2);
    assert_eq!(result.fields[0].name, "tag");
    assert_eq!(result.fields[1].name, "payload");
    assert!(!result.methods.is_empty());
    let flags = defs.iter().find(|d| d.full_name == "Permissions").expect("Permissions");
    assert!(flags.is_value_type);
    assert!(flags.fields.is_empty());
}
