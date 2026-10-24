use rusty_c::*;
use std::collections::HashMap;

#[test]
fn evaluates_main_printf() {
    let source = r#"
#include <stdio.h>
int main(void) {
    int x = 1 + 2;
    printf("%d\n", x);
    return 0;
}
"#;
    let mut env = HashMap::new();
    let result = evaluate_c_source(source, &mut env).expect("eval");
    assert_eq!(result.to_string_value(), "3");
}
