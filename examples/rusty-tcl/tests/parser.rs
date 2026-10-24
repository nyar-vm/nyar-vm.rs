use rusty_tcl::script::{TclCommand, TclScript};

#[test]
fn parse_if_while() {
    let script = TclScript::parse("set x 1\nif {$x > 0} { puts ok }").expect("parse");
    assert_eq!(script.commands.len(), 2);
}

#[test]
fn parse_for_foreach_proc() {
    let script =
        TclScript::parse("proc add {a b} { expr {$a + $b} }\nfor {set i 0} {$i < 2} {incr i} { puts $i }\nforeach x {a b} { puts $x }")
            .expect("parse");
    assert!(matches!(script.commands[0], TclCommand::Proc { .. }));
    assert!(matches!(script.commands[1], TclCommand::For { .. }));
    assert!(matches!(script.commands[2], TclCommand::Foreach { .. }));
}
