use rusty_c::{CScript, *};

#[test]
fn parse_main_printf() {
    let source = r#"
#include <stdio.h>
int main(void) {
    int x = 1 + 2;
    printf("%d\n", x);
    return 0;
}
"#;
    let script = CScript::parse(source).expect("parse");
    assert_eq!(script.items.len(), 1);
}
