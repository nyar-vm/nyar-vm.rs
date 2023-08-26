use super::{MirLowerer, MirOperand, MirOperation};
use crate::{ValkyrieCompiler, types::hir::ValkyrieType};

#[test]
fn same_method_names_keep_declared_result_types() {
    let hir = ValkyrieCompiler::default()
        .compile_source(
            r#"
structure BooleanOwner {
    micro read() -> bool { return true }
}
structure IntegerOwner {
    micro read() -> i64 { return 7 }
}
micro read_boolean() -> bool { return BooleanOwner.read() }
micro read_integer() -> i64 { return IntegerOwner.read() }
"#,
        )
        .expect("同名不同 owner 的静态方法必须完成 HIR 解析");
    let mir = MirLowerer::lower_module_semantic(&hir);
    for (owner, expected) in [
        ("BooleanOwner.read", ValkyrieType::Boolean),
        ("IntegerOwner.read", ValkyrieType::Integer64 { signed: true }),
    ] {
        let mut matches = 0;
        for function in &mir.functions {
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                if let MirOperation::Call { callee: MirOperand::Symbol(symbol), arguments } = &instruction.kind {
                    if symbol.to_string() == owner {
                        matches += 1;
                        assert!(arguments.is_empty(), "无 self 声明不得增加接收者");
                        assert_eq!(instruction.results.len(), 1, "非 unit 返回值必须绑定结果");
                        assert_eq!(function.value_types.get(&instruction.results[0]), Some(&expected));
                    }
                }
            }
        }
        assert_eq!(matches, 1, "调用必须保持完整 owner 身份");
    }
}
