use rusty_valkyrie::frontend::ParseError;

#[test]
fn from_oak_preserves_source_offset_as_span() {
    let error = oak_core::OakError::new(oak_core::OakErrorKind::SyntaxError {
        message: "expected identifier".to_string(),
        offset: 12,
        source_id: None,
    });
    let parsed = ParseError::from_oak(error);
    match parsed {
        ParseError::Invalid { span: Some(span), .. } => assert_eq!(span, 12..13),
        other => panic!("expected invalid span, got {other:?}"),
    }
}
