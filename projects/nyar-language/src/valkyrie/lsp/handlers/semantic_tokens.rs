use crate::handlers;
use crate::state::{DocumentState, ServerState};
use oak_lsp::types::*;
use oak_valkyrie::ast::*;

/// 语义标记处理器
pub struct SemanticTokensHandler;

impl SemanticTokensHandler {
    pub async fn handle_full(state: &ServerState, uri: &str) -> Option<SemanticTokens> {
        let doc = state.get_document(uri)?;
        if handlers::awsl::document::is_awsl_uri(uri) {
            let mut tokens = Vec::new();
            handlers::awsl::AwslSemanticTokensHandler::collect(&doc, &mut tokens);
            return Some(Self::encode_tokens(&doc, tokens));
        }
        let ast = doc.ast.as_ref()?;

        let mut tokens = Vec::new();
        Self::collect_tokens_from_items(&ast.items, &doc, &mut tokens);
        Some(Self::encode_tokens(&doc, tokens))
    }

    fn encode_tokens(_doc: &DocumentState, mut tokens: Vec<SemanticToken>) -> SemanticTokens {
        tokens.sort_by(|a, b| a.delta_line.cmp(&b.delta_line).then(a.delta_start.cmp(&b.delta_start)));
        let mut last_line = 0;
        let mut last_start = 0;
        let mut data = Vec::new();
        for token in tokens {
            let delta_line = token.delta_line - last_line;
            let delta_start = if delta_line == 0 { token.delta_start - last_start } else { token.delta_start };
            data.push(SemanticToken {
                delta_line,
                delta_start,
                length: token.length,
                token_type: token.token_type,
                token_modifiers_bitset: token.token_modifiers_bitset,
            });
            last_line = token.delta_line;
            last_start = token.delta_start;
        }
        SemanticTokens { result_id: None, data }
    }

    pub async fn handle_range(state: &ServerState, uri: &str, _range: core::range::Range<usize>) -> Option<SemanticTokens> {
        // TODO: 实现按范围收集。目前先返回全量，客户端通常能处理
        Self::handle_full(state, uri).await
    }

    fn collect_tokens_from_items(items: &[Item], doc: &DocumentState, tokens: &mut Vec<SemanticToken>) {
        for item in items {
            match item {
                Item::Micro(m) => {
                    let pos = doc.offset_to_position(m.name.span.start);
                    tokens.push(SemanticToken {
                        delta_line: pos.line,
                        delta_start: pos.character,
                        length: m.name.name.len() as u32,
                        token_type: 3,
                        token_modifiers_bitset: 0,
                    });
                    Self::collect_tokens_from_statements(&m.body.statements, doc, tokens);
                }
                Item::Class(c) => {
                    let pos = doc.offset_to_position(c.name.span.start);
                    tokens.push(SemanticToken {
                        delta_line: pos.line,
                        delta_start: pos.character,
                        length: c.name.name.len() as u32,
                        token_type: 0,
                        token_modifiers_bitset: 0,
                    });
                    Self::collect_tokens_from_items(&c.items, doc, tokens);
                }
                Item::Namespace(n) => {
                    for part in &n.name.parts {
                        let pos = doc.offset_to_position(part.span.start);
                        tokens.push(SemanticToken {
                            delta_line: pos.line,
                            delta_start: pos.character,
                            length: part.name.len() as u32,
                            token_type: 2,
                            token_modifiers_bitset: 0,
                        });
                    }
                    Self::collect_tokens_from_items(&n.items, doc, tokens);
                }
                Item::Widget(w) => {
                    let pos = doc.offset_to_position(w.name.span.start);
                    tokens.push(SemanticToken {
                        delta_line: pos.line,
                        delta_start: pos.character,
                        length: w.name.name.len() as u32,
                        token_type: 0,
                        token_modifiers_bitset: 0,
                    });
                    Self::collect_tokens_from_items(&w.items, doc, tokens);
                }
                Item::TypeFunction(tf) => {
                    let pos = doc.offset_to_position(tf.name.span.start);
                    tokens.push(SemanticToken {
                        delta_line: pos.line,
                        delta_start: pos.character,
                        length: tf.name.name.len() as u32,
                        token_type: 3,
                        token_modifiers_bitset: 0,
                    });
                    Self::collect_tokens_from_statements(&tf.body.statements, doc, tokens);
                }
                Item::Using(u) => {
                    for part in &u.path.parts {
                        let pos = doc.offset_to_position(part.span.start);
                        tokens.push(SemanticToken {
                            delta_line: pos.line,
                            delta_start: pos.character,
                            length: part.name.len() as u32,
                            token_type: 2,
                            token_modifiers_bitset: 0,
                        });
                    }
                }
                Item::Statement(s) => {
                    Self::collect_tokens_from_statement(s, doc, tokens);
                }
                Item::Flags(f) => {
                    let pos = doc.offset_to_position(f.name.span.start);
                    tokens.push(SemanticToken {
                        delta_line: pos.line,
                        delta_start: pos.character,
                        length: f.name.name.len() as u32,
                        token_type: 0,
                        token_modifiers_bitset: 0,
                    });
                }
                Item::Enums(e) => {
                    let pos = doc.offset_to_position(e.name.span.start);
                    tokens.push(SemanticToken {
                        delta_line: pos.line,
                        delta_start: pos.character,
                        length: e.name.name.len() as u32,
                        token_type: 0,
                        token_modifiers_bitset: 0,
                    });
                }
                Item::Trait(t) => {
                    let pos = doc.offset_to_position(t.name.span.start);
                    tokens.push(SemanticToken {
                        delta_line: pos.line,
                        delta_start: pos.character,
                        length: t.name.name.len() as u32,
                        token_type: 2,
                        token_modifiers_bitset: 0,
                    });
                }
                Item::Variant(v) => {
                    let pos = doc.offset_to_position(v.name.span.start);
                    tokens.push(SemanticToken {
                        delta_line: pos.line,
                        delta_start: pos.character,
                        length: v.name.name.len() as u32,
                        token_type: 3,
                        token_modifiers_bitset: 0,
                    });
                }
                Item::Effect(e) => {
                    let pos = doc.offset_to_position(e.name.span.start);
                    tokens.push(SemanticToken {
                        delta_line: pos.line,
                        delta_start: pos.character,
                        length: e.name.name.len() as u32,
                        token_type: 3,
                        token_modifiers_bitset: 0,
                    });
                }
                Item::TemplateText { .. } => {}
                Item::TemplateControl { .. } => {}
                Item::TemplateInterpolation { .. } => {}
                Item::Structure(s) => {
                    let pos = doc.offset_to_position(s.name.span.start);
                    tokens.push(SemanticToken {
                        delta_line: pos.line,
                        delta_start: pos.character,
                        length: s.name.name.len() as u32,
                        token_type: 0,
                        token_modifiers_bitset: 0,
                    });
                }
                Item::Singleton(s) => {
                    let pos = doc.offset_to_position(s.name.span.start);
                    tokens.push(SemanticToken {
                        delta_line: pos.line,
                        delta_start: pos.character,
                        length: s.name.name.len() as u32,
                        token_type: 0,
                        token_modifiers_bitset: 0,
                    });
                }
                Item::Property(p) => {
                    let pos = doc.offset_to_position(p.name.span.start);
                    tokens.push(SemanticToken {
                        delta_line: pos.line,
                        delta_start: pos.character,
                        length: p.name.name.len() as u32,
                        token_type: 5,
                        token_modifiers_bitset: 0,
                    });
                }
            }
        }
    }

    fn collect_tokens_from_statements(statements: &[Statement], doc: &DocumentState, tokens: &mut Vec<SemanticToken>) {
        for stmt in statements {
            Self::collect_tokens_from_statement(stmt, doc, tokens);
        }
    }

    fn collect_tokens_from_statement(stmt: &Statement, doc: &DocumentState, tokens: &mut Vec<SemanticToken>) {
        match stmt {
            Statement::Let { pattern, expr, .. } => {
                if let Pattern::Variable { name, .. } = pattern {
                    let pos = doc.offset_to_position(name.span.start);
                    tokens.push(SemanticToken {
                        delta_line: pos.line,
                        delta_start: pos.character,
                        length: name.name.len() as u32,
                        token_type: 4,
                        token_modifiers_bitset: 0,
                    });
                }
                Self::collect_tokens_from_expression(expr, doc, tokens);
            }
            Statement::ExprStmt { expr, .. } => {
                Self::collect_tokens_from_expression(expr, doc, tokens);
            }
        }
    }

    fn collect_tokens_from_expression(expr: &Expr, doc: &DocumentState, tokens: &mut Vec<SemanticToken>) {
        match expr {
            Expr::Ident(id) => {
                let pos = doc.offset_to_position(id.span.start);
                tokens.push(SemanticToken {
                    delta_line: pos.line,
                    delta_start: pos.character,
                    length: id.name.len() as u32,
                    token_type: 4, // variable
                    token_modifiers_bitset: 0,
                });
            }
            Expr::Path(path) => {
                for part in &path.parts {
                    let pos = doc.offset_to_position(part.span.start);
                    tokens.push(SemanticToken {
                        delta_line: pos.line,
                        delta_start: pos.character,
                        length: part.name.len() as u32,
                        token_type: 4, // variable
                        token_modifiers_bitset: 0,
                    });
                }
            }
            Expr::Call { callee, args, .. } => {
                Self::collect_tokens_from_expression(callee, doc, tokens);
                for arg in args {
                    Self::collect_tokens_from_expression(arg, doc, tokens);
                }
            }
            Expr::Binary { left, right, .. } => {
                Self::collect_tokens_from_expression(left, doc, tokens);
                Self::collect_tokens_from_expression(right, doc, tokens);
            }
            Expr::Unary { expr, .. } => {
                Self::collect_tokens_from_expression(expr, doc, tokens);
            }
            Expr::If { condition, then_branch, else_branch, .. } => {
                Self::collect_tokens_from_expression(condition, doc, tokens);
                Self::collect_tokens_from_statements(&then_branch.statements, doc, tokens);
                if let Some(eb) = else_branch {
                    Self::collect_tokens_from_statements(&eb.statements, doc, tokens);
                }
            }
            Expr::Block(b) => {
                Self::collect_tokens_from_statements(&b.statements, doc, tokens);
            }
            _ => {}
        }
    }
}
