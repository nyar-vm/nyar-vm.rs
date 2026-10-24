use rusty_tcl::script::tokenize_words;

#[test]
fn tokenizes_puts_and_set() {
    let cmds = tokenize_words("set x 1\nputs $x");
    assert_eq!(cmds.len(), 2);
    assert_eq!(cmds[0][0].text, "set");
}

#[test]
fn tokenizes_command_substitution() {
    let cmds = tokenize_words("puts [expr {1 + 2}]");
    assert_eq!(cmds[0][1].text, "[expr {1 + 2}]");
}
