use oak_von::language::value::{VonField, VonObject};

use nyar_language::formatter::{FormatError, FormatOptions, PrintStyle, ToDocument};
use nyar_language::printer::{print_document, print_valkyrie, print_valkyrie_source, print_von, render_document, to_document};
use rusty_valkyrie::frontend::parse_source;
use oak_von::language::value::VonValue;

#[test]
fn von_print_via_registry() {
    let value = VonValue::Object(VonObject { fields: vec![VonField { name: "x".into(), value: VonValue::Number(1.0) }] });
    let out = print_von(&value, PrintStyle::Compact, &FormatOptions::default()).unwrap();
    assert_eq!(out, "{x=1}");
    let pretty = print_von(&value, PrintStyle::Indented, &FormatOptions::default()).unwrap();
    assert!(pretty.contains('x'));
}

#[test]
fn valkyrie_source_print_via_oak() {
    let out = print_valkyrie_source("micro main(){let x=1}", PrintStyle::Compact, &FormatOptions::default()).unwrap();
    assert!(out.contains("micro main()"));
    assert!(out.contains("let x=1"));
}

#[test]
fn valkyrie_ast_print_produces_document() {
    let root = parse_source("micro main(){let x=1}").expect("parse");
    let doc = to_document(&root).expect("to_document");
    let compact = render_document(&doc, PrintStyle::Compact, &FormatOptions::default());
    assert!(compact.contains("micro main()"));
    let via_trait = root.to_document().expect("trait to_document");
    let again = render_document(&via_trait, PrintStyle::Compact, &FormatOptions::default());
    assert_eq!(compact, again);
    let via_registry = print_valkyrie(&root, PrintStyle::Compact, &FormatOptions::default()).unwrap();
    assert_eq!(compact, via_registry);
}

#[test]
fn wrong_document_type() {
    let value = VonValue::Null;
    let err = print_document("wat", &value, PrintStyle::Indented, &FormatOptions::default()).unwrap_err();
    assert!(matches!(err, FormatError::WrongDocument { .. }));
}
