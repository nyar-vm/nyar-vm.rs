use super::{AstTraverser, DocumentState, ServerState, SymbolInfo, VisitorContext};
use oak_lsp::types::LocationRange;
use oak_valkyrie::ast::{Expr, Item, Statement};

/// 符号查找访问器
pub struct SymbolFinder<'a> {
    pub state: &'a ServerState,
}

impl<'a> SymbolFinder<'a> {
    pub fn new(state: &'a ServerState) -> Self {
        Self { state }
    }
}

#[async_recursion::async_recursion]
impl<'a> super::AstVisitor<SymbolInfo> for SymbolFinder<'a> {
    async fn visit_item(&self, item: &Item, context: &VisitorContext) -> Option<SymbolInfo> {
        match item {
            Item::Using(u) => {
                for part in &u.path.parts {
                    if context.offset >= part.span.start && context.offset <= part.span.end {
                        let ns_so_far: String = u.path.parts.iter().take_while(|p| p.span.start <= part.span.start).map(|p| p.name.clone()).collect::<Vec<_>>().join("::");
                        if let Some(global) = self.state.resolve_namespace_or_symbol(&ns_so_far, context.uri, context.context_ns) {
                            return Some(SymbolInfo {
                                name: global.name.clone(),
                                namespace: global.namespace.clone(),
                                kind: format!("{:?}", global.kind),
                                type_info: None,
                                documentation: global.documentation.clone(),
                                location: LocationRange { uri: global.uri.clone().into(), range: global.range.clone() },
                                signature: None,
                                class_info: None,
                            });
                        }
                    }
                }
                for import in &u.imports {
                    if context.offset >= import.span.start && context.offset <= import.span.end {
                        let ns = u.path.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::");
                        if let Some(ns_map) = self.state.index.symbols.get(&ns) {
                            if let Some(global) = ns_map.get(&import.name) {
                                return Some(SymbolInfo {
                                    name: global.name.clone(),
                                    namespace: global.namespace.clone(),
                                    kind: format!("{:?}", global.kind),
                                    type_info: None,
                                    documentation: global.documentation.clone(),
                                    location: LocationRange { uri: global.uri.clone().into(), range: global.range.clone() },
                                    signature: None,
                                    class_info: None,
                                });
                            }
                        }
                    }
                }
                None
            }
            Item::Micro(m) => {
                if context.offset >= m.name.span.start && context.offset <= m.name.span.end {
                    let signature = ServerState::extract_micro_signature(m);
                    let type_info = ServerState::format_function_signature(&signature);
                    return Some(SymbolInfo {
                        name: m.name.name.clone(),
                        namespace: context.context_ns.to_string(),
                        kind: "function".to_string(),
                        type_info: Some(type_info),
                        documentation: None,
                        location: LocationRange { uri: context.uri.to_string().into(), range: m.span.clone() },
                        signature: Some(signature),
                        class_info: None,
                    });
                }
                if context.offset >= m.body.span.start && context.offset <= m.body.span.end {
                    return AstTraverser::traverse_statements(&m.body.statements, self, context).await;
                }
                None
            }
            Item::TypeFunction(tf) => {
                if context.offset >= tf.name.span.start && context.offset <= tf.name.span.end {
                    let signature = ServerState::extract_type_function_signature(tf);
                    let type_info = ServerState::format_function_signature(&signature);
                    return Some(SymbolInfo {
                        name: tf.name.name.clone(),
                        namespace: context.context_ns.to_string(),
                        kind: "function".to_string(),
                        type_info: Some(type_info),
                        documentation: None,
                        location: LocationRange { uri: context.uri.to_string().into(), range: tf.span.clone() },
                        signature: Some(signature),
                        class_info: None,
                    });
                }
                if context.offset >= tf.body.span.start && context.offset <= tf.body.span.end {
                    return AstTraverser::traverse_statements(&tf.body.statements, self, context).await;
                }
                None
            }
            Item::Class(c) => {
                if context.offset >= c.name.span.start && context.offset <= c.name.span.end {
                    let class_info = ServerState::extract_class_info(c);
                    let mut type_info = format!("class {}", c.name.name);
                    if let Some(ref parent) = class_info.parent_class {
                        type_info = format!("{} : {}", type_info, parent);
                    }
                    return Some(SymbolInfo {
                        name: c.name.name.clone(),
                        namespace: context.context_ns.to_string(),
                        kind: "class".to_string(),
                        type_info: Some(type_info),
                        documentation: None,
                        location: LocationRange { uri: context.uri.to_string().into(), range: c.span.clone() },
                        signature: None,
                        class_info: Some(class_info),
                    });
                }
                return AstTraverser::traverse_items(&c.items, self, context).await;
            }
            Item::Statement(s) => {
                self.visit_statement(s, context).await
            }
            _ => None,
        }
    }
    
    async fn visit_expr(&self, expr: &Expr, context: &VisitorContext) -> Option<SymbolInfo> {
        match expr {
            Expr::Ident(id) => {
                if context.offset >= id.span.start && context.offset <= id.span.end {
                    if let Some(global) = self.state.resolve_symbol(&id.name, context.uri, context.context_ns, context.context_symbol) {
                        return Some(SymbolInfo {
                            name: global.name.clone(),
                            namespace: global.namespace.clone(),
                            kind: format!("{:?}", global.kind),
                            type_info: None,
                            documentation: global.documentation.clone(),
                            location: LocationRange { uri: global.uri.clone().into(), range: global.range.clone() },
                            signature: None,
                            class_info: None,
                        });
                    }
                }
                None
            }
            Expr::Path(path) => {
                for part in &path.parts {
                    if context.offset >= part.span.start && context.offset <= part.span.end {
                        if let Some(global) = self.state.resolve_symbol(&part.name, context.uri, context.context_ns, context.context_symbol) {
                            return Some(SymbolInfo {
                                name: global.name.clone(),
                                namespace: global.namespace.clone(),
                                kind: format!("{:?}", global.kind),
                                type_info: None,
                                documentation: global.documentation.clone(),
                                location: LocationRange { uri: global.uri.clone().into(), range: global.range.clone() },
                                signature: None,
                                class_info: None,
                            });
                        }
                    }
                }
                None
            }
            Expr::Call { callee, args, .. } => {
                if let Some(a) = self.visit_expr(callee, context).await {
                    return Some(a);
                }
                for arg in args {
                    if let Some(a) = self.visit_expr(arg, context).await {
                        return Some(a);
                    }
                }
                None
            }
            Expr::Field { receiver, field, .. } => {
                if let Some(object) = self.visit_expr(receiver, context).await {
                    return Some(object);
                }

                if context.offset >= field.span.start && context.offset <= field.span.end {
                    if let Some(base_info) = self.state.get_expression_type(receiver, context.uri, context.context_ns, context.context_symbol) {
                        if let Some(member) = self.state.find_member_in_type(&base_info, &field.name, context.uri) {
                            return Some(member);
                        }
                    }

                    return Some(SymbolInfo {
                        name: field.name.clone(),
                        namespace: context.context_ns.to_string(),
                        kind: "member".to_string(),
                        type_info: Some(format!("member {}", field.name)),
                        documentation: None,
                        location: LocationRange { uri: context.uri.to_string().into(), range: field.span.clone() },
                        signature: None,
                        class_info: None,
                    });
                }
                None
            }
            Expr::Binary { left, right, .. } => {
                if let Some(a) = self.visit_expr(left, context).await {
                    return Some(a);
                }
                self.visit_expr(right, context).await
            }
            Expr::Unary { expr, .. } => {
                self.visit_expr(expr, context).await
            }
            Expr::If { condition, then_branch, else_branch, .. } => {
                if let Some(a) = self.visit_expr(condition, context).await {
                    return Some(a);
                }
                if let Some(a) = AstTraverser::traverse_statements(&then_branch.statements, self, context).await {
                    return Some(a);
                }
                if let Some(eb) = else_branch {
                    return AstTraverser::traverse_statements(&eb.statements, self, context).await;
                }
                None
            }
            Expr::Block(b) => {
                AstTraverser::traverse_statements(&b.statements, self, context).await
            }
            _ => None,
        }
    }
    
    async fn visit_statement(&self, stmt: &Statement, context: &VisitorContext) -> Option<SymbolInfo> {
        match stmt {
            Statement::Let { expr, span, .. } => {
                if context.offset >= span.start && context.offset <= span.end {
                    return AstTraverser::traverse_expr(expr, self, context).await;
                }
                None
            }
            Statement::ExprStmt { expr, span, .. } => {
                if context.offset >= span.start && context.offset <= span.end {
                    return AstTraverser::traverse_expr(expr, self, context).await;
                }
                None
            }
        }
    }
}