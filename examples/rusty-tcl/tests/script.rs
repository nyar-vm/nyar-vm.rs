use rusty_tcl::script::{TclCommand, TclScript};

#[test]
fn parse_puts() {
    let script = TclScript::parse("puts hello").expect("parse");
    assert!(matches!(script.commands[0], TclCommand::Puts { .. }));
}

#[test]
fn parse_set_and_incr() {
    let script = TclScript::parse("set x 1\nincr x").expect("parse");
    assert_eq!(script.commands.len(), 2);
}

#[test]
fn parse_proc_foreach_and_list() {
    let script = TclScript::parse("proc add {a b} { expr {$a + $b} }\nforeach x {a b} { puts $x }\nset xs [list a b]\n").expect("parse");
    assert!(matches!(script.commands[0], TclCommand::Proc { .. }));
    assert!(matches!(script.commands[1], TclCommand::Foreach { .. }));
    assert!(matches!(script.commands[2], TclCommand::Set { .. }));
}
