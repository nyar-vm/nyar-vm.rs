use nyar_language::formatter::normalize_extensions;

#[test]
fn normalize_extensions_strips_dot() {
    assert_eq!(normalize_extensions(&[".awsl".into(), "v".into()]), vec!["awsl", "v"]);
}
