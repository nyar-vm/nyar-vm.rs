use super::{AstTraverser, DocumentState, ServerState, SymbolInfo, VisitorContext};
use oak_lsp::types::LocationRange;
use oak_valkyrie::ast::{Expr, Item, Pattern, Statement};

/// 标识符查找访问器
pub struct IdentifierFinder<'a> {
    pub state: &'a ServerState,
}

impl<'a> IdentifierFinder<'a> {
    pub fn new(state: &'a ServerState) -> Self {
        Self { state }
    }
}

#[async_recursion::async_recursion]
impl<'a> super::AstVisitor<SymbolInfo> for IdentifierFinder<'a> {
    async fn visit_item(&self, item: &Item, context: &VisitorContext) -> Option<SymbolInfo> {
        match item {
            Item::Namespace(n) => {
                for part in &n.name.parts {
                    if context.offset >= part.span.start && context.offset <= part.span.end {
                        return Some(SymbolInfo {
                            name: part.name.clone(),
                            namespace: context.context_ns.to_string(),
                            kind: "namespace".to_string(),
                            type_info: Some(format!("namespace {}", part.name)),
                            documentation: None,
                            location: LocationRange { uri: context.uri.to_string().into(), range: part.span.clone() },
                            signature: None,
                            class_info: None,
                        });
                    }
                }
                for nested_item in &n.items {
                    if let Some(symbol) = self.visit_item(nested_item, context).await {
                        return Some(symbol);
                    }
                }
                None
            }
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
                        return Some(SymbolInfo {
                            name: part.name.clone(),
                            namespace: context.context_ns.to_string(),
                            kind: "module".to_string(),
                            type_info: Some(format!("module {}", ns_so_far)),
                            documentation: Some("*未找到定义*".to_string()),
                            location: LocationRange { uri: context.uri.to_string().into(), range: part.span.clone() },
                            signature: None,
                            class_info: None,
                        });
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
                        return Some(SymbolInfo {
                            name: import.name.clone(),
                            namespace: ns.clone(),
                            kind: "unknown".to_string(),
                            type_info: Some(format!("{}::{}", ns, import.name)),
                            documentation: Some("*未找到定义*".to_string()),
                            location: LocationRange { uri: context.uri.to_string().into(), range: import.span.clone() },
                            signature: None,
                            class_info: None,
                        });
                    }
                }
                None
            }
            Item::Class(c) => {
                if context.offset >= c.name.span.start && context.offset <= c.name.span.end {
                    return Some(SymbolInfo {
                        name: c.name.name.clone(),
                        namespace: context.context_ns.to_string(),
                        kind: "class".to_string(),
                        type_info: Some(format!("class {}", c.name.name)),
                        documentation: None,
                        location: LocationRange { uri: context.uri.to_string().into(), range: c.span.clone() },
                        signature: None,
                        class_info: Some(ServerState::extract_class_info(c)),
                    });
                }
                for parent in &c.parents {
                    for part in &parent.name.parts {
                        if context.offset >= part.span.start && context.offset <= part.span.end {
                            let full_path: String = parent.name.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::");
                            if let Some(global) = self.state.resolve_symbol(&full_path, context.uri, context.context_ns, Some(&c.name.name)) {
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
                            return Some(SymbolInfo {
                                name: part.name.clone(),
                                namespace: context.context_ns.to_string(),
                                kind: "type".to_string(),
                                type_info: Some(full_path),
                                documentation: Some("*未找到定义*".to_string()),
                                location: LocationRange { uri: context.uri.to_string().into(), range: part.span.clone() },
                                signature: None,
                                class_info: None,
                            });
                        }
                    }
                }
                for nested_item in &c.items {
                    if let Some(symbol) = self.visit_item(nested_item, context).await {
                        return Some(symbol);
                    }
                }
                None
            }
            Item::Micro(m) => {
                if context.offset >= m.name.span.start && context.offset <= m.name.span.end {
                    let signature = ServerState::extract_micro_signature(m);
                    return Some(SymbolInfo {
                        name: m.name.name.clone(),
                        namespace: context.context_ns.to_string(),
                        kind: "function".to_string(),
                        type_info: Some(ServerState::format_function_signature(&signature)),
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
                    return Some(SymbolInfo {
                        name: tf.name.name.clone(),
                        namespace: context.context_ns.to_string(),
                        kind: "function".to_string(),
                        type_info: Some(ServerState::format_function_signature(&signature)),
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
            Item::Trait(t) => {
                if context.offset >= t.name.span.start && context.offset <= t.name.span.end {
                    return Some(SymbolInfo {
                        name: t.name.name.clone(),
                        namespace: context.context_ns.to_string(),
                        kind: "interface".to_string(),
                        type_info: Some(format!("trait {}", t.name.name)),
                        documentation: None,
                        location: LocationRange { uri: context.uri.to_string().into(), range: t.span.clone() },
                        signature: None,
                        class_info: None,
                    });
                }
                for method in &t.methods {
                    if context.offset >= method.span.start && context.offset <= method.span.end {
                        if context.offset >= method.name.span.start && context.offset <= method.name.span.end {
                            let signature = ServerState::extract_function_signature(method);
                            return Some(SymbolInfo {
                                name: method.name.name.clone(),
                                namespace: context.context_ns.to_string(),
                                kind: "method".to_string(),
                                type_info: Some(ServerState::format_function_signature(&signature)),
                                documentation: None,
                                location: LocationRange { uri: context.uri.to_string().into(), range: method.span.clone() },
                                signature: Some(signature),
                                class_info: None,
                            });
                        }
                    }
                }
                None
            }
            Item::Enums(e) => {
                if context.offset >= e.name.span.start && context.offset <= e.name.span.end {
                    return Some(SymbolInfo {
                        name: e.name.name.clone(),
                        namespace: context.context_ns.to_string(),
                        kind: "enum".to_string(),
                        type_info: Some(format!("enum {}", e.name.name)),
                        documentation: None,
                        location: LocationRange { uri: context.uri.to_string().into(), range: e.span.clone() },
                        signature: None,
                        class_info: None,
                    });
                }
                for variant in &e.variants {
                    if context.offset >= variant.span.start && context.offset <= variant.span.end {
                        if context.offset >= variant.name.span.start && context.offset <= variant.name.span.end {
                            return Some(SymbolInfo {
                                name: variant.name.name.clone(),
                                namespace: context.context_ns.to_string(),
                                kind: "enum_member".to_string(),
                                type_info: Some(format!("{}::{}", e.name.name, variant.name.name)),
                                documentation: None,
                                location: LocationRange { uri: context.uri.to_string().into(), range: variant.span.clone() },
                                signature: None,
                                class_info: None,
                            });
                        }
                    }
                }
                None
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
                    return Some(SymbolInfo {
                        name: id.name.clone(),
                        namespace: context.context_ns.to_string(),
                        kind: "unknown".to_string(),
                        type_info: Some(id.name.clone()),
                        documentation: Some("*未找到定义*".to_string()),
                        location: LocationRange { uri: context.uri.to_string().into(), range: id.span.clone() },
                        signature: None,
                        class_info: None,
                    });
                }
                None
            }
            Expr::Path(path) => {
                for part in &path.parts {
                    if context.offset >= part.span.start && context.offset <= part.span.end {
                        let full_path: String = path.parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join("::");
                        if let Some(global) = self.state.resolve_symbol(&full_path, context.uri, context.context_ns, context.context_symbol) {
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
                        return Some(SymbolInfo {
                            name: part.name.clone(),
                            namespace: context.context_ns.to_string(),
                            kind: "unknown".to_string(),
                            type_info: Some(full_path),
                            documentation: Some("*未找到定义*".to_string()),
                            location: LocationRange { uri: context.uri.to_string().into(), range: part.span.clone() },
                            signature: None,
                            class_info: None,
                        });
                    }
                }
                None
            }
            Expr::Call { callee, args, .. } => {
                if let Some(symbol) = self.visit_expr(callee, context).await {
                    return Some(symbol);
                }
                for arg in args {
                    if let Some(symbol) = self.visit_expr(arg, context).await {
                        return Some(symbol);
                    }
                }
                None
            }
            Expr::Field { receiver, field, .. } => {
                if let Some(symbol) = self.visit_expr(receiver, context).await {
                    return Some(symbol);
                }
                if context.offset >= field.span.start && context.offset <= field.span.end {
                    return Some(SymbolInfo {
                        name: field.name.clone(),
                        namespace: context.context_ns.to_string(),
                        kind: "member".to_string(),
                        type_info: Some(format!(".{}", field.name)),
                        documentation: None,
                        location: LocationRange { uri: context.uri.to_string().into(), range: field.span.clone() },
                        signature: None,
                        class_info: None,
                    });
                }
                None
            }
            Expr::Binary { left, right, .. } => {
                if let Some(symbol) = self.visit_expr(left, context).await {
                    return Some(symbol);
                }
                self.visit_expr(right, context).await
            }
            Expr::Unary { expr, .. } => {
                self.visit_expr(expr, context).await
            }
            Expr::If { condition, then_branch, else_branch, .. } => {
                if let Some(symbol) = self.visit_expr(condition, context).await {
                    return Some(symbol);
                }
                if let Some(symbol) = AstTraverser::traverse_statements(&then_branch.statements, self, context).await {
                    return Some(symbol);
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
            Statement::Let { pattern, expr, span, .. } => {
                if let Pattern::Variable { name, span: pattern_span } = pattern {
                    if context.offset >= pattern_span.start && context.offset <= pattern_span.end {
                        return Some(SymbolInfo {
                            name: name.name.clone(),
                            namespace: context.context_ns.to_string(),
                            kind: "variable".to_string(),
                            type_info: None,
                            documentation: None,
                            location: LocationRange { uri: context.uri.to_string().into(), range: span.clone() },
                            signature: None,
                            class_info: None,
                        });
                    }
                }
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