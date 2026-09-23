use super::{DocumentState, ServerState, SymbolInfo};
use oak_valkyrie::ast::{Expr, Item, Statement};

/// 访问器上下文
pub struct VisitorContext<'a> {
    pub uri: &'a str,
    pub doc: &'a DocumentState,
    pub offset: usize,
    pub context_ns: &'a str,
    pub context_symbol: Option<&'a str>,
}

/// AST 遍历访问器 trait
pub trait AstVisitor<T> {
    /// 访问 Item 节点
    async fn visit_item(&self, item: &Item, context: &VisitorContext) -> Option<T>;
    
    /// 访问 Expr 节点
    async fn visit_expr(&self, expr: &Expr, context: &VisitorContext) -> Option<T>;
    
    /// 访问 Statement 节点
    async fn visit_statement(&self, stmt: &Statement, context: &VisitorContext) -> Option<T>;
}

/// 通用的 AST 遍历器
pub struct AstTraverser;

impl AstTraverser {
    /// 遍历 Items，使用指定的访问器
    pub async fn traverse_items<T, V: AstVisitor<T>>(
        items: &[Item],
        visitor: &V,
        context: &VisitorContext,
    ) -> Option<T> {
        for item in items {
            if let Some(result) = visitor.visit_item(item, context).await {
                return Some(result);
            }
        }
        None
    }
    
    /// 遍历表达式，使用指定的访问器
    pub async fn traverse_expr<T, V: AstVisitor<T>>(
        expr: &Expr,
        visitor: &V,
        context: &VisitorContext,
    ) -> Option<T> {
        visitor.visit_expr(expr, context).await
    }
    
    /// 遍历语句，使用指定的访问器
    pub async fn traverse_statements<T, V: AstVisitor<T>>(
        statements: &[Statement],
        visitor: &V,
        context: &VisitorContext,
    ) -> Option<T> {
        for stmt in statements {
            if let Some(result) = visitor.visit_statement(stmt, context).await {
                return Some(result);
            }
        }
        None
    }
}