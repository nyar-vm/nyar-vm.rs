use rusty_valkyrie::frontend::{DIAG_IDENTIFIER_NOT_SNAKE_CASE, parse_source, validate_snake_case};

#[test]
fn rejects_camel_case_let_micro_and_param() {
    let source = r#"micro demo(onClick: i32) -> i32 {
    let fooBar = 1;
    return onClick;
}
"#;
    let root = parse_source(source).expect("fixture should parse");
    let violations = validate_snake_case(&root);
    let names: Vec<_> = violations.iter().map(|v| v.name.as_str()).collect();
    assert!(names.contains(&"onClick"));
    assert!(names.contains(&"fooBar"));
    assert!(violations.iter().all(|v| v.code == DIAG_IDENTIFIER_NOT_SNAKE_CASE));
}

#[test]
fn rejects_camel_case_in_widget_script() {
    let source = r#"widget counter {
    let themeChange = 0;

    micro onTap() {
        themeChange = themeChange + 1;
    }

    micro defaultActive() -> bool {
        return true;
    }
}"#;
    let root = parse_source(source).expect("widget fixture should parse");
    let violations = validate_snake_case(&root);
    let names: Vec<_> = violations.iter().map(|v| v.name.as_str()).collect();
    assert!(names.contains(&"themeChange"));
    assert!(names.contains(&"onTap"));
    assert!(names.contains(&"defaultActive"));
}

#[test]
fn accepts_valid_snake_case_names() {
    let source = r#"micro on_click(item_count: i32) -> i32 {
    let theme_change = item_count;
    return theme_change;
}
"#;
    let root = parse_source(source).expect("valid fixture should parse");
    let violations = validate_snake_case(&root);
    assert!(violations.is_empty(), "{violations:?}");
}
