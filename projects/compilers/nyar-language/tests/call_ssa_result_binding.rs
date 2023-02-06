use nyar_language::{MirLowerer, MirOperation, ValkyrieCompiler, types::SourceID};

#[test]
fn call_instructions_bind_one_ssa_result() {
    let source = r#"
namespace demo.overflow;

micro clamped_mul_add(base: i64, step: i64) -> i64 {
    let lo: i64 = -2147483648
    let hi: i64 = 2147483647
    let mut acc: i64 = base
    let mut n: i64 = step
    while n != 0 {
        let bound: i64 = if lo % 10 != 0 { lo / 10 - 1 } else { lo / 10 }
        if acc < bound + 1 || acc > hi / 10 {
            return 0
        }
        let digit: i64 = n % 10
        acc = acc * 10 + digit
        n = n / 10
    }
    return acc
}
"#;
    let hir = ValkyrieCompiler::new(SourceID { version_id: 1 }).compile_source(source).expect("hir");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let func = mir.functions.iter().find(|f| f.symbol.contains("clamped_mul_add")).expect("clamped_mul_add");
    for block in &func.blocks {
        for insn in &block.instructions {
            if matches!(&insn.kind, MirOperation::Call { .. }) {
                assert_eq!(insn.results.len(), 1, "call must bind one SSA result: {:?}", insn.kind);
            }
        }
    }
}
