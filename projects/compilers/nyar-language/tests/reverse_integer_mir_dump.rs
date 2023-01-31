use nyar_language::{MirLowerer, MirOperation, ValkyrieCompiler, types::SourceID};

#[test]
fn dump_reverse_integer_mir() {
    let source = r#"
namespace leetcode.reverse_integer;

micro reverse(x: i64) -> i64 {
    let mi: i64 = -2147483648
    let mx: i64 = 2147483647
    let mut n: i64 = x
    let mut ans: i64 = 0
    while n != 0 {
        if ans < mi / 10 + 1 || ans > mx / 10 {
            return 0
        }
        let mut y: i64 = n % 10
        if n < 0 && y > 0 {
            y = y - 10
        }
        ans = ans * 10 + y
        n = (n - y) / 10
    }
    return ans
}
"#;
    let hir = ValkyrieCompiler::new(SourceID { version_id: 1 }).compile_source(source).expect("hir");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let func = mir.functions.iter().find(|f| f.symbol.contains("reverse")).expect("reverse");
    for block in &func.blocks {
        for insn in &block.instructions {
            if matches!(&insn.kind, MirOperation::Call { .. }) {
                assert_eq!(insn.results.len(), 1, "call must bind one SSA result: {:?}", insn.kind);
            }
        }
    }
}
