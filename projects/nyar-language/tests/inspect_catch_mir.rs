use nyar_language::{MirLowerer, SourceID, ValkyrieCompiler};
use std::{fs, io::Write};

#[test]
fn inspect_catch_function_raise_mir() {
    let source = r#"
micro effect_f() -> i32 {
    raise 42
}

micro main() -> ExitCode {
    let result: i32 = catch effect_f() {
        case msg:
            resume msg
    }
    return ExitCode(result)
}
"#;
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9400 }).compile_source(source).expect("compile");
    let module = MirLowerer::lower_module(&hir);
    let mut out = String::new();
    for function in &module.functions {
        out.push_str(&format!("=== Function: {} ===\n", function.symbol));
        out.push_str(&format!("Entry: {:?}\n", function.entry));
        out.push_str(&format!("Return type: {:?}\n", function.return_type));
        for (i, block) in function.blocks.iter().enumerate() {
            out.push_str(&format!("  Block {} (id={:?}, label={}):\n", i, block.id, block.label));
            for instr in &block.instructions {}
            out.push_str(&format!("    Terminator: {:?}\n", block.terminator));
            out.push_str(&format!("    Parameters: {:?}\n", block.parameters));
        }
        out.push_str(&format!("  Suspend points: {}\n", function.suspend_points.len()));
        for sp in &function.suspend_points {
            out.push_str(&format!("    state_id={}, effect={:?}, resume_target={:?}\n", sp.state_id, sp.effect, sp.resume_target));
        }
        out.push_str(&format!("  Continuations: {}\n", function.continuations.len()));
        for cont in &function.continuations {
            out.push_str(&format!("    dispatch={:?}, resume={:?}, exit={:?}\n", cont.dispatch_block, cont.resume_target, cont.handler_exit));
        }
        out.push_str(&format!("  Value types:\n"));
        for (k, v) in &function.value_types {
            out.push_str(&format!("    {:?}: {:?}\n", k, v));
        }
    }
    let _ = fs::write("e:\\Goddess of Victory\\valkyrie.rs\\mir_dump.txt", &out);
    // also print
    print!("{}", out);
    let _ = std::io::stdout().flush();
}
