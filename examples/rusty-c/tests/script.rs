use rusty_c::{CError, CScript};

#[test]
fn parse_minimal_main() {
    let script = CScript::parse("int main(void) { return 0; }").expect("parse");
    assert_eq!(script.items.len(), 1);
}

#[test]
fn rejects_empty() {
    assert_eq!(CScript::parse("   ").unwrap_err(), CError::EmptyScript);
}
