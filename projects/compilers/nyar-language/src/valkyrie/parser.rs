//! Valkyrie 文本解析入口。
//!
//! 正式编译流只能调用 [`parse`] / [`parse_source`]，不得引入第二条 parser 成功路径。

pub use crate::valkyrie::frontend::{self, ValkyrieRoot, parse_source, std_range};
pub use oak_core::OakDiagnostics;

use oak_core::{Builder, ParseSession, SourceText};
use oak_valkyrie::{ValkyrieBuilder, ValkyrieLanguage};

/// Oak 前端返回的解析结果（含 diagnostics）。
pub type ParseOutput = OakDiagnostics<frontend::ast::ValkyrieRoot>;

/// 使用 Oak 解析并构造 Valkyrie AST 根节点。
pub fn parse(source: &str) -> ParseOutput {
    let language = ValkyrieLanguage::default();
    let builder = ValkyrieBuilder::new(&language);
    let text = SourceText::new(source);
    let mut session = ParseSession::<ValkyrieLanguage>::default();
    builder.build(&text, &[], &mut session)
}

#[cfg(test)]
mod tests {
    use super::{frontend, parse};

    #[test]
    fn parses_minimal_micro_with_oak() {
        let output = parse("micro main() -> i32 { return 1 }");
        let root = output.result.expect("Oak 应构造 Valkyrie AST");
        assert!(!root.items.is_empty());
    }

    #[test]
    fn preserves_call_and_numeric_expression_structure() {
        let output = parse("micro main() -> i32 { let value = add(1, 2); return value }");
        let root = output.result.expect("Oak 应构造调用 AST");
        let frontend::ast::StatementNode::Micro(micro) = &root.items[0]
        else {
            panic!("缺少 micro 声明");
        };
        let frontend::ast::Statement::Let(binding) = &micro.body.statements[0]
        else {
            panic!("缺少 let 绑定");
        };
        let frontend::ast::TermExpression::ApplyCall { callee, args, .. } = &binding.expr
        else {
            panic!("调用没有保留为 Oak ApplyCall");
        };
        assert!(matches!(callee.as_ref(), frontend::ast::TermExpression::NamePath(_)));
        assert!(matches!(args[0], frontend::ast::TermExpression::IntegerLiteral { .. }));
        assert!(matches!(args[1], frontend::ast::TermExpression::IntegerLiteral { .. }));
    }

    #[test]
    fn parses_generic_and_structure_declarations() {
        let output = parse("class Box[T] { value: T } micro identity[T](value: T) -> T { return value }");
        let root = output.result.expect("Oak 应构造泛型和结构 AST");
        assert!(root.items.iter().any(|item| matches!(item, frontend::ast::StatementNode::Class(_))));
        assert!(root.items.iter().any(|item| matches!(item, frontend::ast::StatementNode::Micro(_))));
    }
}
